[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string]$ServiceExecutable,
    [Parameter(Mandatory = $true)] [string]$SpeakerBroker,
    [Parameter(Mandatory = $true)] [string]$MicrophoneBroker,
    [string]$DisplayConfig = '',
    [string]$DisplayControlUserSid = '',
    [Parameter(Mandatory = $true)] [AllowEmptyString()] [string]$ExpectedOriginalImagePath,
    [switch]$InstallIfMissing,
    [Parameter(Mandatory = $true)] [string]$EvidenceRoot,
    [string]$AudioRouteControlUserSid = '',
    [string]$CameraIngressUserSid = '',
    [Parameter(Mandatory)][ValidateScript({ $address=$null; [Net.IPAddress]::TryParse($_,[ref]$address) -and $address.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork -and $_ -match '^\d+\.\d+\.\d+\.\d+$' })][string]$LocalIpv4,
    [Parameter(Mandatory)][ValidateScript({ $address=$null; [Net.IPAddress]::TryParse($_,[ref]$address) -and $address.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork -and $_ -match '^\d+\.\d+\.\d+\.\d+$' })][string]$AndroidIpv4
)

$ErrorActionPreference = 'Stop'
$serviceName = 'CapyIOBroker'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Unified service update requires an elevated Administrator process.'
}
if ($DisplayControlUserSid -and $DisplayControlUserSid -notmatch '^S-1-(?:\d+-)+\d+$') {
    throw 'Display control SID is invalid.'
}
if ($AudioRouteControlUserSid -and $AudioRouteControlUserSid -notmatch '^S-1-(?:\d+-)+\d+$') {
    throw 'Audio Route control SID is invalid.'
}
if ($CameraIngressUserSid -and $CameraIngressUserSid -notmatch '^S-1-(?:\d+-)+\d+$') {
    throw 'Camera ingress SID is invalid.'
}
$serviceExe = (Resolve-Path -LiteralPath $ServiceExecutable -ErrorAction Stop).Path
$speakerExe = (Resolve-Path -LiteralPath $SpeakerBroker -ErrorAction Stop).Path
$microphoneExe = (Resolve-Path -LiteralPath $MicrophoneBroker -ErrorAction Stop).Path
if ([bool]$DisplayConfig -ne [bool]$DisplayControlUserSid) { throw 'Display config and its user SID must be supplied together.' }
$displayConfigPath = if ($DisplayConfig) { (Resolve-Path -LiteralPath $DisplayConfig -ErrorAction Stop).Path } else { $null }
$evidence = [IO.Path]::GetFullPath($EvidenceRoot)
$repositoryTarget = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\target'))
if (-not $evidence.StartsWith($repositoryTarget + [IO.Path]::DirectorySeparatorChar,
        [StringComparison]::OrdinalIgnoreCase)) {
    throw 'EvidenceRoot must remain below repository target/.'
}
New-Item -ItemType Directory -Force -Path $evidence | Out-Null

$original = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
$freshInstall = $null -eq $original
if ($freshInstall -and (-not $InstallIfMissing -or $ExpectedOriginalImagePath)) {
    throw 'New service installation requires -InstallIfMissing and an explicitly empty previous ImagePath.'
}
if (-not $freshInstall -and $original.PathName -ne $ExpectedOriginalImagePath) {
    throw 'Installed service image path changed after preflight; refusing to update.'
}
$originalWasRunning = -not $freshInstall -and $original.State -eq 'Running'
$backup = [ordered]@{
    capturedAtUtc = [DateTime]::UtcNow.ToString('o')
    existed = -not $freshInstall
    name = $serviceName
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
    '--microphone-peer-port', '40110'
)
if ($displayConfigPath) {
    $newArguments += @('--single-display-config', $displayConfigPath, '--display-control-user-sid', $DisplayControlUserSid)
}
if ($AudioRouteControlUserSid) {
    $newArguments += @('--audio-route-control-user-sid', $AudioRouteControlUserSid)
}
if ($CameraIngressUserSid) {
    $newArguments += @('--camera-ingress-user-sid', $CameraIngressUserSid)
}
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
$stoppedByUs = $false
try {
    if ($originalWasRunning) {
        Invoke-ServiceControl -Arguments @('stop', $serviceName) | Out-Null
        Wait-ServiceState 'Stopped' 30 | Out-Null
        $stoppedByUs = $true
    }
    if ($freshInstall) {
        Invoke-ServiceControl -Arguments @('create', $serviceName, 'binPath=', $newImagePath, 'start=', 'demand', 'obj=', 'LocalSystem') | Out-Null
    } else {
        Invoke-ServiceControl -Arguments @('config', $serviceName, 'binPath=', $newImagePath) | Out-Null
    }
    $updated = $true
    Invoke-ServiceControl -Arguments @('start', $serviceName) | Out-Null
    $active = Wait-ServiceState 'Running' 30
    $result = [ordered]@{
        capturedAtUtc = [DateTime]::UtcNow.ToString('o')
        passed = $true
        state = $active.State
        serviceSha256 = (Get-FileHash -LiteralPath $serviceExe -Algorithm SHA256).Hash
        displayConfigSha256 = if ($displayConfigPath) { (Get-FileHash -LiteralPath $displayConfigPath -Algorithm SHA256).Hash } else { $null }
        displayControlUserSid = $DisplayControlUserSid
        audioRouteControlUserSid = $AudioRouteControlUserSid
        cameraIngressUserSid = $CameraIngressUserSid
        localIpv4 = $LocalIpv4
        androidIpv4 = $AndroidIpv4
    }
    $result | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidence 'result.json') -Encoding UTF8
} catch {
    $failure = $_.Exception.Message
    if ($updated -or $stoppedByUs) {
        try {
            & sc.exe stop $serviceName 2>&1 | Out-Null
            Wait-ServiceState 'Stopped' 30 | Out-Null
            if ($freshInstall) {
                Invoke-ServiceControl -Arguments @('delete', $serviceName) | Out-Null
            } elseif ($updated) {
                Invoke-ServiceControl -Arguments @('config', $serviceName, 'binPath=', $original.PathName) | Out-Null
            }
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
