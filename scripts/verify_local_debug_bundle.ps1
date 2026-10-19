[CmdletBinding()]
param([Parameter(Mandatory)][string]$PackageRoot, [switch]$AllowRollbackBackups)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root = Get-Item -LiteralPath $PackageRoot
if (-not $root.PSIsContainer -or ($root.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Package root must be an ordinary directory.' }
$manifest = Get-Content -LiteralPath (Join-Path $root.FullName 'manifest.json') -Raw | ConvertFrom-Json
if ($manifest.schemaVersion -ne 3 -or $manifest.kind -ne 'capyio-local-debug-bundle' -or $manifest.productionRelease -ne $false -or $manifest.distribution -ne 'local-debug-only') { throw 'Not a supported local-debug bundle.' }
$required = @('capyio-desktop.exe','capyio-windows-service.exe','capyio-native-virtual-microphone.exe','capyio-native-virtual-speaker.exe','capyio-display-engine-worker.exe','libcapyio_display_engine.dll','capyio-ptp-adb-lab.exe','capyio-avc-lab-receiver.exe','capyio-camera-session-host.exe','capyio-camera-virtual-lab.exe','CapyIO.ViGEmX360Sidecar.exe','Nefarius.ViGEm.Client.dll','android/CapyIO-debug.apk','README.md','LICENSE','verify_local_debug_bundle.ps1','update_local_debug_bundle.ps1')
$profile = if ($manifest.PSObject.Properties['profile']) { [string]$manifest.profile } else { 'integrated' }
if ($profile -notin @('core','integrated')) { throw 'Unsupported application profile.' }
$coreRequired = @('capyio-desktop.exe','capyio-windows-service.exe','capyio-native-virtual-microphone.exe','capyio-native-virtual-speaker.exe','capyio-ptp-adb-lab.exe','capyio-avc-lab-receiver.exe','capyio-camera-session-host.exe','capyio-camera-virtual-lab.exe','android/CapyIO-debug.apk','README.md','LICENSE','verify_local_debug_bundle.ps1','update_local_debug_bundle.ps1','install_local_debug_bundle.ps1','remove_local_debug_bundle.ps1','check_local_prerequisites.ps1')
if ($profile -eq 'core') { $required = $coreRequired }
$seen=@{}
if ($profile -eq 'integrated') { $required += @('tools/sunshine_wgc_capture.exe','assets/apps.json','include/common.hlsl','assets/shaders/directx/convert_yuv420_packed_uv_type0_ps.hlsl') }
$resourcePattern = '^(tools/sunshine_wgc_capture\.exe|assets/apps\.json|assets/shaders/directx/[A-Za-z0-9_-]+\.hlsl|include/[A-Za-z0-9_-]+\.hlsl)$'
foreach ($entry in $manifest.artifacts) {
    $name=[string]$entry.file
    if ($profile -eq 'core' -and $coreRequired -cnotcontains $name) { throw "Not a Core artifact: $name" }
    if (($name -notmatch '^[A-Za-z0-9_+.-]+$' -and $name -cne 'android/CapyIO-debug.apk' -and $name -notmatch $resourcePattern) -or $name -in @('.','..','manifest.json') -or $seen.ContainsKey($name)) { throw "Invalid package path: $name" }
    if ($required -cnotcontains $name -and $name -cnotin @('check_local_prerequisites.ps1','install_local_debug_bundle.ps1','remove_local_debug_bundle.ps1') -and $name -notmatch $resourcePattern -and $name -notmatch '^[A-Za-z0-9_+.-]+\.dll$') { throw "Unexpected artifact: $name" }
    if ($entry.sha256 -notmatch '^[0-9a-fA-F]{64}$') { throw 'Missing SHA256.' }
    $path=$root.FullName
    foreach ($part in $name.Split('/')) {
        $path=Join-Path $path $part
        if ((Get-Item -LiteralPath $path).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Package entries cannot be links.' }
    }
    $file=Get-Item -LiteralPath $path
    if ($file.PSIsContainer -or $file.Length -ne $entry.bytes -or (Get-FileHash -LiteralPath $path).Hash -ne $entry.sha256) { throw "Integrity mismatch: $name" }
    $seen[$name]=$true
}
foreach ($name in $required) { if (-not $seen.ContainsKey($name)) { throw "Incomplete package: $name" } }
foreach ($item in Get-ChildItem -LiteralPath $root.FullName -Force -Recurse) {
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Package contains a link.' }
    $relative = $item.FullName.Substring($root.FullName.TrimEnd('\','/').Length + 1).Replace('\','/')
    if ($AllowRollbackBackups -and ($relative -eq '.capyio-backups' -or $relative.StartsWith('.capyio-backups/'))) { continue }
    if ($item.PSIsContainer) {
        $relative = $item.FullName.Substring($root.FullName.TrimEnd('\','/').Length + 1).Replace('\','/')
        if ($relative -cnotin @('android','tools','include','assets','assets/shaders','assets/shaders/directx')) { throw 'Unexpected package directory.' }
    } else {
        $relative = $item.FullName.Substring($root.FullName.TrimEnd('\','/').Length + 1).Replace('\','/')
        if ($relative -cne 'manifest.json' -and -not $seen.ContainsKey($relative)) { throw "Unlisted file: $relative" }
    }
}
if ($profile -eq 'core') {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $apk = [IO.Compression.ZipFile]::OpenRead((Join-Path $root.FullName 'android/CapyIO-debug.apk'))
    try {
        $native = @($apk.Entries | Where-Object { $_.FullName -like 'lib/*.so' } | ForEach-Object FullName)
        if ($native.Count -ne 1 -or $native[0] -cne 'lib/arm64-v8a/libcapyio_android_jni.so') { throw 'Core APK contains unexpected native libraries.' }
    } finally { $apk.Dispose() }
}
Write-Output "Integrity PASS: $($seen.Count) files. Not a publisher signature or hardware acceptance."
Write-Output "ONLY application entry: $(Join-Path $root.FullName 'capyio-desktop.exe')"
