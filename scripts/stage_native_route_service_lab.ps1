[CmdletBinding()]
param(
    [switch]$Deploy,
    [Parameter(Mandatory)][ValidatePattern('^S-1-5-21-\d+-\d+-\d+-\d+$')][string]$ControlUserSid,
    [Parameter(Mandatory)][ValidateScript({ $address=$null; [Net.IPAddress]::TryParse($_,[ref]$address) -and $address.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork -and $_ -match '^\d+\.\d+\.\d+\.\d+$' })][string]$LocalIpv4,
    [Parameter(Mandatory)][ValidateScript({ $address=$null; [Net.IPAddress]::TryParse($_,[ref]$address) -and $address.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork -and $_ -match '^\d+\.\d+\.\d+\.\d+$' })][string]$AndroidIpv4
)

# Approved local-lab handoff, not a public installer. Existing driver and
# credentials stay untouched; the service updater records and restores config.
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$sid = $ControlUserSid
$previous = Join-Path $repo 'target/integrated-service-staging/0.5.11'
$files = @('capyio-windows-service.exe', 'capyio-native-virtual-speaker.exe',
    'capyio-native-virtual-microphone.exe', 'capyio-display-engine-worker.exe')
foreach ($file in $files) {
    if (-not (Test-Path -LiteralPath (Join-Path $repo "target/debug/$file") -PathType Leaf)) {
        throw "Build the matching service/helper first: $file"
    }
}
if (-not $Deploy) {
    Write-Output 'Plan: stage four matching debug binaries, preserve display credentials/library, update only the owned display worker hash, enable SID-scoped audio Route and display control for the explicitly selected user, restart CapyIOBroker with rollback evidence. No drivers are changed.'
    Write-Output 'Run with -Deploy from elevated PowerShell only after stopping hardware use.'
    return
}
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
if (-not [Security.Principal.WindowsPrincipal]::new($identity).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Deployment requires an elevated Administrator process.'
}
$service = Get-CimInstance Win32_Service -Filter "Name='CapyIOBroker'"
if (-not $service -or -not $service.PathName.StartsWith((Join-Path $previous 'capyio-windows-service.exe') + ' ', [StringComparison]::OrdinalIgnoreCase) -or
    -not $service.PathName.EndsWith('--display-control-user-sid ' + $sid, [StringComparison]::Ordinal)) {
    throw 'The inspected lab service configuration changed; no update applied.'
}
$stamp = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-ffff')
$stage = Join-Path $repo "target/integrated-service-staging/route-$stamp"
New-Item -ItemType Directory -Path $stage | Out-Null
# The native engine resolves shader assets/includes and the WGC helper from
# the worker's runtime directory, not only from the loaded engine DLL folder.
foreach ($directory in @('assets','include','tools')) {
    Copy-Item -LiteralPath (Join-Path $previous "display-session/$directory") -Destination (Join-Path $stage $directory) -Recurse
}
foreach ($file in $files) { Copy-Item -LiteralPath (Join-Path $repo "target/debug/$file") -Destination $stage }
$config = Get-Content -LiteralPath (Join-Path $previous 'single-display-runtime-config.json') -Raw | ConvertFrom-Json
$config.engineLaunch.workerExecutablePath = Join-Path $stage 'capyio-display-engine-worker.exe'
$config.engineLaunch.workerSha256 = (Get-FileHash -LiteralPath $config.engineLaunch.workerExecutablePath -Algorithm SHA256).Hash.ToLowerInvariant()
$configPath = Join-Path $stage 'single-display-runtime-config.json'
[IO.File]::WriteAllText($configPath, ($config | ConvertTo-Json -Depth 10), [Text.UTF8Encoding]::new($false))
$arguments = @{
    LocalIpv4 = $LocalIpv4
    AndroidIpv4 = $AndroidIpv4
    ServiceExecutable = Join-Path $stage 'capyio-windows-service.exe'
    SpeakerBroker = Join-Path $stage 'capyio-native-virtual-speaker.exe'
    MicrophoneBroker = Join-Path $stage 'capyio-native-virtual-microphone.exe'
    DisplayConfig = $configPath
    DisplayControlUserSid = $sid
    AudioRouteControlUserSid = $sid
    ExpectedOriginalImagePath = $service.PathName
    EvidenceRoot = Join-Path $stage 'deployment-evidence'
}
& (Join-Path $PSScriptRoot 'update_integrated_capyio_service_lab.ps1') @arguments
Write-Output "Deployed local-lab service: $stage"
