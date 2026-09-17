[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string]$ServiceExecutable,
    [Parameter(Mandatory = $true)] [string]$SpeakerBroker,
    [Parameter(Mandatory = $true)] [string]$MicrophoneBroker,
    [Parameter(Mandatory = $true)] [string]$DisplayConfig,
    [Parameter(Mandatory = $true)] [string]$DisplayControlUserSid,
    [Parameter(Mandatory = $true)] [string]$ExpectedOriginalImagePath,
    [Parameter(Mandatory = $true)] [string]$EvidenceRoot,
    [string]$LocalIpv4 = '100.66.231.100',
    [string]$AndroidIpv4 = '100.110.59.35'
)

$ErrorActionPreference = 'Stop'
$serviceName = 'CapyIOBroker'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Unified service update requires an elevated Administrator process.'
}
if ($DisplayControlUserSid -notmatch '^S-1-(?:\d+-)+\d+$') {
    throw 'Display control SID is invalid.'
}
$serviceExe = (Resolve-Path -LiteralPath $ServiceExecutable -ErrorAction Stop).Path
$speakerExe = (Resolve-Path -LiteralPath $SpeakerBroker -ErrorAction Stop).Path
$microphoneExe = (Resolve-Path -LiteralPath $MicrophoneBroker -ErrorAction Stop).Path
$displayConfigPath = (Resolve-Path -LiteralPath $DisplayConfig -ErrorAction Stop).Path
$evidence = [IO.Path]::GetFullPath($EvidenceRoot)
$repositoryTarget = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\target'))
if (-not $evidence.StartsWith($repositoryTarget + [IO.Path]::DirectorySeparatorChar,
        [StringComparison]::OrdinalIgnoreCase)) {
    throw 'EvidenceRoot must remain below repository target/.'
}
New-Item -ItemType Directory -Force -Path $evidence | Out-Null

$original = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
if ($null -eq $original) { throw "$serviceName is not installed." }
if ($original.PathName -ne $ExpectedOriginalImagePath) {
    throw 'Installed service image path changed after preflight; refusing to update.'
}
$originalWasRunning = $original.State -eq 'Running'
$backup = [ordered]@{
    capturedAtUtc = [DateTime]::UtcNow.ToString('o')
    name = $original.Name
    state = $original.State
    startMode = $original.StartMode
    startName = $original.StartName
    imagePath = $original.PathName
}
$backup | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidence 'original-service.json') -Encoding UTF8

$newArguments = @(
    '--mode', 'native-speaker',
    '--broker', $speakerExe,
    '--bind-ip', $LocalIpv4,
    '--port', '40101',
    '--peer-ip', $AndroidIpv4,
    '--peer-port', '40100',
    '--microphone-broker', $microphoneExe,
    '--microphone-bind-ip', $LocalIpv4,
    '--microphone-port', '40111',
    '--microphone-peer-ip', $AndroidIpv4,
    '--microphone-peer-port', '40110',
    '--single-display-config', $displayConfigPath,
    '--display-control-user-sid', $DisplayControlUserSid
)
$quote = { param([string]$Value) '"' + $Value.Replace('"', '\"') + '"' }
$newImagePath = (& $quote $serviceExe) + ' ' + (($newArguments | ForEach-Object { & $quote $_ }) -join ' ')

function Invoke-ServiceControl([string[]]$Arguments) {
    $output = @(& sc.exe @Arguments 2>&1 | ForEach-Object { $_.ToString() })
    if ($LASTEXITCODE -ne 0) { throw "sc.exe failed: $($Arguments[0])" }
    return $output
}

function Wait-ServiceState([string]$State, [int]$Seconds) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        $current = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
        if ($current.State -eq $State) { return $current }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Service did not reach $State."
}

$updated = $false
try {
    if ($originalWasRunning) {
        Invoke-ServiceControl -Arguments @('stop', $serviceName) | Out-Null
        Wait-ServiceState 'Stopped' 30 | Out-Null
    }
    Invoke-ServiceControl -Arguments @('config', $serviceName, 'binPath=', $newImagePath) | Out-Null
    $updated = $true
    Invoke-ServiceControl -Arguments @('start', $serviceName) | Out-Null
    $active = Wait-ServiceState 'Running' 30
    $result = [ordered]@{
        capturedAtUtc = [DateTime]::UtcNow.ToString('o')
        passed = $true
        state = $active.State
        serviceSha256 = (Get-FileHash -LiteralPath $serviceExe -Algorithm SHA256).Hash
        displayConfigSha256 = (Get-FileHash -LiteralPath $displayConfigPath -Algorithm SHA256).Hash
        displayControlUserSid = $DisplayControlUserSid
        localIpv4 = $LocalIpv4
        androidIpv4 = $AndroidIpv4
    }
    $result | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidence 'result.json') -Encoding UTF8
} catch {
    $failure = $_.Exception.Message
    if ($updated) {
        try {
            & sc.exe stop $serviceName 2>&1 | Out-Null
            Wait-ServiceState 'Stopped' 30 | Out-Null
            Invoke-ServiceControl -Arguments @('config', $serviceName, 'binPath=', $original.PathName) | Out-Null
            if ($originalWasRunning) {
                Invoke-ServiceControl -Arguments @('start', $serviceName) | Out-Null
                Wait-ServiceState 'Running' 30 | Out-Null
            }
        } catch {
            $failure += '; rollback failed: ' + $_.Exception.Message
        }
    }
    [ordered]@{
        capturedAtUtc = [DateTime]::UtcNow.ToString('o')
        passed = $false
        failure = $failure
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidence 'result.json') -Encoding UTF8
    throw $failure
}
