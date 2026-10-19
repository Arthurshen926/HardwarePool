$ErrorActionPreference='Stop'
# Filesystem-only fake artifacts. No binary is executed, no device/service is changed.
$work=Join-Path ([IO.Path]::GetTempPath()) ('capyio-bundle-test-' + [Guid]::NewGuid().ToString('N'))
$package=Join-Path $work 'package'
$target=Join-Path $work 'application'
New-Item -ItemType Directory -Path (Join-Path $package 'android'),$target | Out-Null
$names=@('capyio-desktop.exe','capyio-windows-service.exe','capyio-native-virtual-microphone.exe','capyio-native-virtual-speaker.exe','capyio-display-engine-worker.exe','libcapyio_display_engine.dll','sunshine_wgc_capture.exe','capyio-ptp-adb-lab.exe','capyio-avc-lab-receiver.exe','capyio-camera-session-host.exe','capyio-camera-virtual-lab.exe','CapyIO.ViGEmX360Sidecar.exe','Nefarius.ViGEm.Client.dll','android/CapyIO-debug.apk','README.md','LICENSE','verify_local_debug_bundle.ps1','update_local_debug_bundle.ps1')
$names = @($names | Where-Object { $_ -ne 'sunshine_wgc_capture.exe' }) + @('tools/sunshine_wgc_capture.exe','assets/apps.json','include/common.hlsl','assets/shaders/directx/convert_yuv420_packed_uv_type0_ps.hlsl')
$artifacts=@(foreach($name in $names) {
    $file=Join-Path $package $name
    New-Item -ItemType Directory -Path (Split-Path -Parent $file) -Force | Out-Null
    Set-Content -LiteralPath $file -Value "fake-$name"
    @{file=$name;bytes=(Get-Item -LiteralPath $file).Length;sha256=(Get-FileHash -LiteralPath $file).Hash}
})
@{schemaVersion=3;kind='capyio-local-debug-bundle';productionRelease=$false;distribution='local-debug-only';artifacts=$artifacts} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $package 'manifest.json')
$old=Join-Path $target 'capyio-desktop.exe'
Set-Content -LiteralPath $old -Value 'old-app'
$oldHash=(Get-FileHash -LiteralPath $old).Hash
& (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $package
& (Join-Path $PSScriptRoot 'update_local_debug_bundle.ps1') -PackageRoot $package -TargetDirectory $target
if ((Get-FileHash -LiteralPath $old).Hash -ne $oldHash -or (Test-Path -LiteralPath (Join-Path $target '.capyio-backups'))) { throw 'Default plan unexpectedly changed files.' }
& (Join-Path $PSScriptRoot 'update_local_debug_bundle.ps1') -PackageRoot $package -TargetDirectory $target -Apply
$backups=@(Get-ChildItem -LiteralPath (Join-Path $target '.capyio-backups') -Directory)
if ($backups.Count -ne 1) { throw 'Expected one rollback snapshot.' }
$backup=$backups[0]
foreach ($resource in @('tools/sunshine_wgc_capture.exe','include/common.hlsl','assets/shaders/directx/convert_yuv420_packed_uv_type0_ps.hlsl')) {
    if (-not (Test-Path -LiteralPath (Join-Path $target $resource))) { throw "Missing upgraded display resource: $resource" }
}
& (Join-Path $PSScriptRoot 'update_local_debug_bundle.ps1') -Mode Rollback -TargetDirectory $target -BackupName $backup.Name -Apply
if ((Get-FileHash -LiteralPath $old).Hash -ne $oldHash) { throw 'Original executable not restored.' }
if (Test-Path -LiteralPath (Join-Path $target 'include/common.hlsl')) { throw 'Added display resource not rolled back.' }
if (Test-Path -LiteralPath (Join-Path $target 'capyio-native-virtual-speaker.exe')) { throw 'New runtime file was not moved into rollback backup.' }
# A complete installed bundle must remain verifiable after runtime-only upgrade.
$managed=Join-Path $work 'managed-application'
& (Join-Path $PSScriptRoot 'install_local_debug_bundle.ps1') -PackageRoot $package -TargetDirectory $managed -Apply
$managedExe=Join-Path $managed 'capyio-desktop.exe'
Set-Content -LiteralPath $managedExe -Value 'previous-runtime'
$managedManifest=Join-Path $managed 'manifest.json'
$oldManifest=Get-Content -LiteralPath $managedManifest -Raw | ConvertFrom-Json
$oldEntry=$oldManifest.artifacts | Where-Object file -eq 'capyio-desktop.exe'
$oldEntry.sha256=(Get-FileHash -LiteralPath $managedExe).Hash
$oldEntry.bytes=(Get-Item -LiteralPath $managedExe).Length
$oldManifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $managedManifest
$manifestHash=(Get-FileHash -LiteralPath $managedManifest).Hash
$apkHash=(Get-FileHash -LiteralPath (Join-Path $managed 'android/CapyIO-debug.apk')).Hash
& (Join-Path $PSScriptRoot 'update_local_debug_bundle.ps1') -PackageRoot $package -TargetDirectory $managed -Apply
& (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $managed -AllowRollbackBackups
if ((Get-FileHash -LiteralPath $managedExe).Hash -ne (Get-FileHash -LiteralPath (Join-Path $package 'capyio-desktop.exe')).Hash) { throw 'Runtime upgrade did not replace the executable.' }
if ((Get-FileHash -LiteralPath (Join-Path $managed 'android/CapyIO-debug.apk')).Hash -ne $apkHash) { throw 'Runtime upgrade changed the APK.' }
$managedBackup=Get-ChildItem -LiteralPath (Join-Path $managed '.capyio-backups') -Directory | Select-Object -First 1
& (Join-Path $PSScriptRoot 'update_local_debug_bundle.ps1') -Mode Rollback -TargetDirectory $managed -BackupName $managedBackup.Name -Apply
if ((Get-FileHash -LiteralPath $managedManifest).Hash -ne $manifestHash) { throw 'Rollback did not restore the exact original manifest.' }
& (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $managed -AllowRollbackBackups
& (Join-Path $PSScriptRoot 'remove_local_debug_bundle.ps1') -TargetDirectory $managed -Apply
$archive=Get-ChildItem -LiteralPath $work -Directory | Where-Object Name -Like 'managed-application.removed-*' | Select-Object -First 1
& (Join-Path $PSScriptRoot 'install_local_debug_bundle.ps1') -PackageRoot $archive.FullName -TargetDirectory $managed -Apply
if ((Get-FileHash -LiteralPath (Join-Path $managed 'manifest.json')).Hash -ne $manifestHash) { throw 'Restore changed the manifest.' }
Set-Content -LiteralPath (Join-Path $package 'libcapyio_display_engine.dll') -Value 'changed'
$rejected=$false
try { & (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $package } catch { $rejected=$true }
if (-not $rejected) { throw 'Changed artifact accepted.' }
Write-Output "PASS: integrity, default plan, upgrade, manifest rollback, installed verification, removal/restore, changed bytes rejected. Fake fixtures retained at $work"
