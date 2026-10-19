[CmdletBinding()]
param([string]$SignedDriverBundle='', [string]$MakeNsis=(Join-Path $env:LOCALAPPDATA 'tauri\NSIS\makensis.exe'))
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent $PSScriptRoot
$buildId='alpha-'+[DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-ffff')
$output=Join-Path $repo ('target\unified-windows\'+$buildId)
$bundle=Join-Path $output 'payload'
New-Item -ItemType Directory -Path $bundle -Force|Out-Null
$files=@('capyio-desktop.exe','capyio-windows-service.exe','capyio-native-virtual-microphone.exe','capyio-native-virtual-speaker.exe','capyio-ptp-adb-lab.exe','capyio-avc-lab-receiver.exe','capyio-camera-session-host.exe','capyio-camera-virtual-lab.exe','capyio_windows_camera_mf.dll')
foreach($file in $files){Copy-Item -LiteralPath (Join-Path $repo ('target\debug\'+$file)) -Destination $bundle}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'manage_windows_installation.ps1') -Destination $bundle
Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination $bundle
Copy-Item -LiteralPath (Join-Path $repo 'THIRD_PARTY_NOTICES.md') -Destination $bundle
Copy-Item -LiteralPath (Join-Path $repo 'docs\UNIFIED_WINDOWS_INSTALLATION.md') -Destination (Join-Path $bundle 'README.md')
foreach($helper in @('windows_driver_payload.ps1','windows_driver_setup.cs')){Copy-Item -LiteralPath (Join-Path $PSScriptRoot $helper) -Destination $bundle}
$hasDrivers=[bool]$SignedDriverBundle
if($hasDrivers){
    . (Join-Path $PSScriptRoot 'windows_driver_payload.ps1')
    $driverManifest=Read-CapyDriverBundle $SignedDriverBundle
    $drivers=Join-Path $bundle 'drivers'
    foreach($file in $driverManifest.files){
        $destination=Join-Path $drivers $file.path
        New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force|Out-Null
        Copy-Item -LiteralPath (Join-Path $SignedDriverBundle $file.path) -Destination $destination
    }
    Copy-Item -LiteralPath (Join-Path $SignedDriverBundle 'driver-bundle.json') -Destination $drivers
    Read-CapyDriverBundle $drivers|Out-Null
}
$artifacts=@(Get-ChildItem -LiteralPath $bundle -File -Recurse|ForEach-Object {@{file=$_.FullName.Substring($bundle.Length+1).Replace('\','/');bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash}})
@{schemaVersion=1;kind='capyio-unified-local-windows';buildId=$buildId;productionRelease=$false;sourceRevision=((& git -C $repo rev-parse HEAD)|Out-String).Trim();sourceTreeDirty=[bool](& git -C $repo status --porcelain);buildProvenance='current-workspace-debug-artifacts';artifacts=$artifacts;signedDriverPayloadIncluded=$hasDrivers;managedComponents=@('application','broker-service','camera-com');externalPrerequisites=$(if($hasDrivers){@()}else{@('audio-driver','audio-apo','vhf-driver')});preserveUserData=$true}|ConvertTo-Json -Depth 6|Set-Content (Join-Path $bundle 'windows-installation.json') -Encoding UTF8
$installer=Join-Path $output ('CapyIO-'+$buildId+'-setup.exe')
& $MakeNsis '/V2' "/DBUNDLE_ROOT=$bundle" "/DOUTPUT_FILE=$installer" "/DBUILD_ID=$buildId" (Join-Path $PSScriptRoot 'unified_windows_installer.nsi')
if($LASTEXITCODE){throw 'NSIS build failed.'}
@{installer=$installer;sha256=(Get-FileHash -LiteralPath $installer).Hash;bytes=(Get-Item $installer).Length;installed=$false;newMachineAcceptance=$false;driverInstallationIncluded=$hasDrivers}|ConvertTo-Json|Set-Content (Join-Path $output 'build-result.json') -Encoding UTF8
Get-Content (Join-Path $output 'build-result.json')
