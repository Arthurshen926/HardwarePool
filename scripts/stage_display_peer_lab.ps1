[CmdletBinding()]
param(
    [switch]$Deploy,
    [Parameter(Mandatory)][ValidatePattern('^S-1-5-21-\d+-\d+-\d+-\d+$')][string]$ControlUserSid,
    [Parameter(Mandatory)][ValidateScript({ $address=$null; [Net.IPAddress]::TryParse($_,[ref]$address) -and $address.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork -and $_ -match '^\d+\.\d+\.\d+\.\d+$' })][string]$LocalIpv4,
    [Parameter(Mandatory)][ValidateScript({ $address=$null; [Net.IPAddress]::TryParse($_,[ref]$address) -and $address.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork -and $_ -match '^\d+\.\d+\.\d+\.\d+$' })][string]$AndroidIpv4,
    [ValidatePattern('^display-[a-z0-9-]+$')][string]$PreviousStageName = 'display-diagnostics-v2-20260929',
    [ValidatePattern('^display-[a-z0-9-]+$')][string]$StageName = 'display-peer-20260929'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$previous = Join-Path $repo "target/integrated-service-staging/$PreviousStageName"
$stage = Join-Path $repo "target/integrated-service-staging/$StageName"
$audioStage = Join-Path $repo 'target/integrated-service-staging/route-20260929-090958-5904'
$sid = $ControlUserSid
$manifestPath = Join-Path $stage 'prepared-stage.json'
$configPath = Join-Path $stage 'single-display-runtime-config.json'

function Assert-Hash([string]$Path, [string]$Expected) {
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ne $Expected) {
        throw "Prepared artifact changed: $Path"
    }
}

if ($Deploy) {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    if (-not [Security.Principal.WindowsPrincipal]::new($identity).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Administrator required for deployment; preparation does not change the service.'
    }
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if (-not $manifest.PSObject.Properties['target'] -or
        $manifest.target.controlUserSid -cne $ControlUserSid -or
        $manifest.target.localIpv4 -cne $LocalIpv4 -or
        $manifest.target.androidIpv4 -cne $AndroidIpv4) {
        throw 'Deployment target differs from prepared target; prepare a new reviewed stage.'
    }
    $service = Get-CimInstance Win32_Service -Filter "Name='CapyIOBroker'"
    if ($null -eq $service -or $service.PathName -cne $manifest.expectedOriginalImagePath) {
        throw 'Installed service ImagePath differs from the exact prepared baseline; refusing deployment.'
    }
    foreach ($artifact in $manifest.artifacts) { Assert-Hash $artifact.path $artifact.sha256 }
    # Application-scoped rules do not follow an EXE into a new stage directory.
    # Limit this controlled-lab deployment to the identified Y700 and video ports.
    $displayConfig = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
    $portBase = [int]$displayConfig.engineLaunch.portBase
    if ($portBase -ne 50000) { throw 'Unexpected display port base; review the lab firewall scope before deployment.' }
    $workerPath = Join-Path $stage 'capyio-display-engine-worker.exe'
    foreach ($transport in @('TCP','UDP')) {
        $ruleName = "CapyIO.DisplayPeer.$StageName.$transport"
        if (Get-NetFirewallRule -Name $ruleName -ErrorAction SilentlyContinue) { throw 'Stage firewall rules already exist; review before retrying deployment.' }
        $ports = if ($transport -eq 'TCP') { @(50021) } else { @(50009,50010) }
        New-NetFirewallRule -Name $ruleName -DisplayName $ruleName -Direction Inbound -Action Allow `
            -Program $workerPath -Protocol $transport -LocalPort $ports -LocalAddress $LocalIpv4 `
            -RemoteAddress $AndroidIpv4 -Profile Any -EdgeTraversalPolicy Block | Out-Null
    }
    & (Join-Path $PSScriptRoot 'update_integrated_capyio_service_lab.ps1') `
        -LocalIpv4 $LocalIpv4 -AndroidIpv4 $AndroidIpv4 `
        -ServiceExecutable (Join-Path $stage 'capyio-windows-service.exe') `
        -SpeakerBroker (Join-Path $audioStage 'capyio-native-virtual-speaker.exe') `
        -MicrophoneBroker (Join-Path $audioStage 'capyio-native-virtual-microphone.exe') `
        -DisplayConfig $configPath -DisplayControlUserSid $sid -AudioRouteControlUserSid $sid `
        -ExpectedOriginalImagePath $manifest.expectedOriginalImagePath `
        -EvidenceRoot (Join-Path $stage 'deployment-evidence')
    return
}

if (Test-Path -LiteralPath $stage) { throw 'Stage already exists. Preserve it; deploy explicitly with -Deploy after review.' }
$service = Get-CimInstance Win32_Service -Filter "Name='CapyIOBroker'"
$expectedExe = Join-Path $previous 'capyio-windows-service.exe'
if ($null -eq $service -or -not (
    $service.PathName.StartsWith('"' + $expectedExe + '" ', [StringComparison]::OrdinalIgnoreCase) -or
    $service.PathName.StartsWith($expectedExe + ' ', [StringComparison]::OrdinalIgnoreCase))) {
    throw 'Installed service is not the expected display-diagnostics-v2 baseline.'
}
Push-Location $repo
try {
    & cargo build --offline -p capyio-windows-service --bins
    if ($LASTEXITCODE -ne 0) { throw 'Service/worker build failed.' }
} finally { Pop-Location }

$config = Get-Content -LiteralPath (Join-Path $previous 'single-display-runtime-config.json') -Raw | ConvertFrom-Json
Assert-Hash $config.engineLaunch.engineLibraryPath $config.engineLaunch.engineSha256
foreach ($directory in @('assets', 'include', 'tools')) {
    if (-not (Test-Path -LiteralPath (Join-Path $previous $directory) -PathType Container)) {
        throw "Missing required display runtime directory: $directory"
    }
}
New-Item -ItemType Directory -Path $stage | Out-Null
foreach ($directory in @('assets', 'include', 'tools')) {
    Copy-Item -LiteralPath (Join-Path $previous $directory) -Destination (Join-Path $stage $directory) -Recurse
}
foreach ($name in @('capyio-windows-service.exe', 'capyio-display-engine-worker.exe')) {
    Copy-Item -LiteralPath (Join-Path $repo "target/debug/$name") -Destination (Join-Path $stage $name)
}
$config.engineLaunch.workerExecutablePath = Join-Path $stage 'capyio-display-engine-worker.exe'
$config.engineLaunch.workerSha256 = (Get-FileHash -LiteralPath $config.engineLaunch.workerExecutablePath -Algorithm SHA256).Hash.ToLowerInvariant()
# Preserve engine paths and legacy display credential references. No credential
# file, and especially no node private key, is read or copied by this script.
[IO.File]::WriteAllText($configPath, ($config | ConvertTo-Json -Depth 12), [Text.UTF8Encoding]::new($false))
$artifacts = @(Get-ChildItem -LiteralPath $stage -File -Recurse | ForEach-Object {
    [ordered]@{ path = $_.FullName; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
})
foreach ($external in @($config.engineLaunch.engineLibraryPath, (Join-Path $audioStage 'capyio-native-virtual-speaker.exe'), (Join-Path $audioStage 'capyio-native-virtual-microphone.exe'))) {
    $artifacts += [ordered]@{ path = $external; sha256 = (Get-FileHash -LiteralPath $external -Algorithm SHA256).Hash }
}
$manifest = [ordered]@{
    preparedAtUtc = [DateTime]::UtcNow.ToString('o')
    expectedOriginalImagePath = $service.PathName
    previousStage = $previous
    target = @{ controlUserSid=$ControlUserSid; localIpv4=$LocalIpv4; androidIpv4=$AndroidIpv4 }
    artifacts = $artifacts
}
[IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
Write-Output "Prepared $stage. Installed service was not stopped or modified. Explicit -Deploy uses the saved exact ImagePath and recoverable updater."
