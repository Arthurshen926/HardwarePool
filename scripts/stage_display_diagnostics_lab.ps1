[CmdletBinding()]
param(
    [switch]$Deploy,
    [Parameter(Mandatory)][ValidatePattern('^S-1-5-21-\d+-\d+-\d+-\d+$')][string]$ControlUserSid,
    [Parameter(Mandatory)][ValidateScript({ $address=$null; [Net.IPAddress]::TryParse($_,[ref]$address) -and $address.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork -and $_ -match '^\d+\.\d+\.\d+\.\d+$' })][string]$LocalIpv4,
    [Parameter(Mandatory)][ValidateScript({ $address=$null; [Net.IPAddress]::TryParse($_,[ref]$address) -and $address.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork -and $_ -match '^\d+\.\d+\.\d+\.\d+$' })][string]$AndroidIpv4
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$previous = Join-Path $repo 'target/integrated-service-staging/display-diagnostics-20260929'
$stage = Join-Path $repo 'target/integrated-service-staging/display-diagnostics-v2-20260929'
$audioStage = Join-Path $repo 'target/integrated-service-staging/route-20260929-090958-5904'
$sid = $ControlUserSid
if (-not $Deploy) { Write-Output 'Updates the existing lab service and display worker only; preserves audio helpers, credentials, engine DLL, and drivers. Requires Administrator.'; return }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
if (-not [Security.Principal.WindowsPrincipal]::new($identity).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Administrator required' }
$service = Get-CimInstance Win32_Service -Filter "Name='CapyIOBroker'"
if (-not $service.PathName.Contains((Join-Path $previous 'capyio-windows-service.exe'))) { throw 'Service stage changed; not updating' }
if (Test-Path -LiteralPath $stage) { throw 'Stage already exists; preserve it for inspection' }
New-Item -ItemType Directory -Path $stage | Out-Null
$runtimeRoot = Join-Path $repo 'target/integrated-service-staging/0.5.11/display-session'
foreach ($directory in @('assets','include','tools')) {
    Copy-Item -LiteralPath (Join-Path $runtimeRoot $directory) -Destination (Join-Path $stage $directory) -Recurse
}
foreach ($name in @('capyio-windows-service.exe','capyio-display-engine-worker.exe')) {
    Copy-Item -LiteralPath (Join-Path $repo "target/debug/$name") -Destination (Join-Path $stage $name)
}
$config = Get-Content -LiteralPath (Join-Path $previous 'single-display-runtime-config.json') -Raw | ConvertFrom-Json
$config.engineLaunch.workerExecutablePath = Join-Path $stage 'capyio-display-engine-worker.exe'
$config.engineLaunch.workerSha256 = (Get-FileHash -LiteralPath $config.engineLaunch.workerExecutablePath).Hash.ToLowerInvariant()
$configPath = Join-Path $stage 'single-display-runtime-config.json'
[IO.File]::WriteAllText($configPath, ($config | ConvertTo-Json -Depth 12), [Text.UTF8Encoding]::new($false))
& (Join-Path $PSScriptRoot 'update_integrated_capyio_service_lab.ps1') `
    -LocalIpv4 $LocalIpv4 -AndroidIpv4 $AndroidIpv4 `
    -ServiceExecutable (Join-Path $stage 'capyio-windows-service.exe') `
    -SpeakerBroker (Join-Path $audioStage 'capyio-native-virtual-speaker.exe') `
    -MicrophoneBroker (Join-Path $audioStage 'capyio-native-virtual-microphone.exe') `
    -DisplayConfig $configPath -DisplayControlUserSid $sid -AudioRouteControlUserSid $sid `
    -ExpectedOriginalImagePath $service.PathName -EvidenceRoot (Join-Path $stage 'deployment-evidence')
