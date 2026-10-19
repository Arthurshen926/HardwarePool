[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ExpectedAndroidSignerSha256,
    [Parameter(Mandatory)][string]$DisplayRuntimeManifest,
    [string]$AndroidSdkRoot = $env:ANDROID_SDK_ROOT,
    [string]$GameStreamStageRoot = 'target/android-moonlight-unified',
    [switch]$AcknowledgeLocalDebugThirdParty,
    [switch]$UseExistingArtifacts
)
# Creates a new local directory only. Never deploys, installs or changes trust.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $AcknowledgeLocalDebugThirdParty) { throw 'Require -AcknowledgeLocalDebugThirdParty: GPL/native display integration is local debug only, not cleared for redistribution.' }
$repo = Split-Path -Parent $PSScriptRoot
if (-not $AndroidSdkRoot) { $AndroidSdkRoot = $env:ANDROID_HOME }
if (-not $AndroidSdkRoot) { throw 'Supply AndroidSdkRoot with apksigner. ADB is not a software runtime dependency.' }
$tools = Get-ChildItem -LiteralPath (Join-Path $AndroidSdkRoot 'build-tools') -Directory | Where-Object Name -Match '^\d+\.\d+\.\d+$' | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if (-not $tools) { throw 'Android build-tools missing.' }
$displayManifest = Get-Item -LiteralPath $DisplayRuntimeManifest
$display = Get-Content -LiteralPath $displayManifest.FullName -Raw | ConvertFrom-Json
if ($display.distribution -ne 'local-debug-only' -or -not $display.artifacts -or -not $display.licenseNotice) { throw 'Display manifest requires distribution=local-debug-only, licenseNotice and artifacts [{file,sha256}].' }
$displaySources = [ordered]@{}
foreach ($item in $display.artifacts) {
    if (($item.file -notmatch '^[A-Za-z0-9_+.-]+\.dll$' -and $item.file -notmatch '^(tools/sunshine_wgc_capture\.exe|assets/apps\.json|assets/shaders/directx/[A-Za-z0-9_-]+\.hlsl|include/[A-Za-z0-9_-]+\.hlsl)$') -or $item.sha256 -notmatch '^[0-9a-fA-F]{64}$' -or $displaySources.Contains($item.file)) { throw 'Display inputs must be DLLs or explicit display runtime resources with SHA256.' }
    $path = Join-Path $displayManifest.DirectoryName $item.file
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $item.sha256) { throw "Display dependency hash mismatch: $($item.file)" }
    $displaySources[$item.file] = $path
}
if (-not $displaySources.Contains('libcapyio_display_engine.dll')) { throw 'Display manifest must include libcapyio_display_engine.dll and all its non-system DLL dependencies.' }
foreach ($required in @('tools/sunshine_wgc_capture.exe','assets/apps.json','include/common.hlsl','assets/shaders/directx/convert_yuv420_packed_uv_type0_ps.hlsl')) {
    if (-not $displaySources.Contains($required)) { throw "Display runtime resource missing: $required" }
}
$oldRevision = $env:CAPYIO_BUILD_REVISION
$oldTime = $env:CAPYIO_BUILD_TIME
Push-Location $repo
try {
    $revision = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE) { throw 'Cannot resolve source revision.' }
    $dirty = [bool](& git status --porcelain --untracked-files=normal)
    $env:CAPYIO_BUILD_REVISION = $revision + $(if ($dirty) { '-dirty' } else { '' })
    $env:CAPYIO_BUILD_TIME = [DateTime]::UtcNow.ToString('o')
    if (-not $UseExistingArtifacts) {
        & pnpm --filter '@capyio/desktop' build
        if ($LASTEXITCODE) { throw 'Shared UI build failed.' }
        & cargo build --offline -p capyio-desktop --bin capyio-desktop --features tauri/custom-protocol
        if ($LASTEXITCODE) { throw 'Embedded desktop build failed.' }
        & cargo build --offline -p capyio-remote-touchpad-adapter -p capyio-vcamdroid-adapter -p capyio-windows-camera-mf -p capyio-windows-service -p capyio-native-audio-lan --bins
        if ($LASTEXITCODE) { throw 'Service/helper build failed.' }
        & (Join-Path $PSScriptRoot 'build_integrated_android_debug.ps1') -GameStreamStageRoot $GameStreamStageRoot -BuildRevision $env:CAPYIO_BUILD_REVISION
    }
    $apk = Join-Path $repo 'platform/android/app/build/outputs/apk/debug/app-debug.apk'
    $validatedApkHash = (Get-FileHash -LiteralPath $apk).Hash
    $certs = @(& (Join-Path $tools.FullName 'apksigner.bat') verify --print-certs $apk 2>&1)
    if ($LASTEXITCODE) { throw 'APK signature verification failed.' }
    $signers = @($certs | ForEach-Object { if ($_ -match '^Signer #\d+ certificate SHA-256 digest: ([0-9a-fA-F]{64})$') { $Matches[1] } })
    if ($signers.Count -ne 1 -or $signers[0] -ne $ExpectedAndroidSignerSha256) { throw 'APK signer differs from expected installed-app identity. Do not uninstall or clear data.' }
    $badging=@(& (Join-Path $tools.FullName 'aapt2.exe') dump badging $apk 2>&1)
    if ($LASTEXITCODE) { throw 'Cannot read APK identity.' }
    $identity=@($badging | Where-Object { $_ -match '^package: ' })
    if ($identity.Count -ne 1 -or $identity[0] -notmatch "^package: name='dev\.capyio\.android' versionCode='(\d+)' versionName='([^']+)' ") { throw 'Not the unified Android application.' }
    $androidVersionCode=$Matches[1]
    $androidVersionName=$Matches[2]
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($apk)
    try {
        foreach ($entry in @('AndroidManifest.xml','classes.dex','assets/capyio-ui/index.html','lib/arm64-v8a/libcapyio_android_jni.so','lib/arm64-v8a/libcapyio-gamestream-client.so')) {
            if (-not $zip.GetEntry($entry)) { throw "Incomplete APK: $entry" }
        }
    } finally { $zip.Dispose() }
    $sources = [ordered]@{}
    foreach ($name in @('capyio-desktop.exe','capyio-windows-service.exe','capyio-native-virtual-microphone.exe','capyio-native-virtual-speaker.exe','capyio-display-engine-worker.exe','capyio-ptp-adb-lab.exe','capyio-avc-lab-receiver.exe','capyio-camera-session-host.exe','capyio-camera-virtual-lab.exe','CapyIO.ViGEmX360Sidecar.exe','Nefarius.ViGEm.Client.dll')) { $sources[$name] = Join-Path $repo "target/debug/$name" }
    foreach ($entry in $displaySources.GetEnumerator()) { if ($sources.Contains($entry.Key)) { throw 'Duplicate runtime filename.' }; $sources[$entry.Key] = $entry.Value }
    $sources['android/CapyIO-debug.apk'] = $apk
    $sources['README.md'] = Join-Path $repo 'docs/LOCAL_DEBUG_UPGRADE.md'
    $sources['LICENSE'] = Join-Path $repo 'LICENSE'
    $sources['verify_local_debug_bundle.ps1'] = Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1'
    $sources['update_local_debug_bundle.ps1'] = Join-Path $PSScriptRoot 'update_local_debug_bundle.ps1'
    $sources['check_local_prerequisites.ps1'] = Join-Path $PSScriptRoot 'check_local_prerequisites.ps1'
    $sources['install_local_debug_bundle.ps1'] = Join-Path $PSScriptRoot 'install_local_debug_bundle.ps1'
    $sources['remove_local_debug_bundle.ps1'] = Join-Path $PSScriptRoot 'remove_local_debug_bundle.ps1'
    foreach ($path in $sources.Values) { if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Required companion missing: $path" } }
    foreach ($path in $sources.Values | Where-Object { $_ -match '\.(exe|dll)$' }) {
        $stream = [IO.File]::OpenRead($path)
        try {
            if ($stream.Length -lt 64 -or $stream.ReadByte() -ne 77 -or $stream.ReadByte() -ne 90) {
                throw "Not a Windows binary (possibly a placeholder): $path"
            }
        } finally { $stream.Dispose() }
    }
    $destination = Join-Path $repo ('target/integrated-test/CapyIO-debug-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-ffff'))
    New-Item -ItemType Directory -Path (Join-Path $destination 'android') | Out-Null
    $artifacts = foreach ($entry in $sources.GetEnumerator()) {
        $source = Get-Item -LiteralPath $entry.Value
        if ($source.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Package inputs cannot be links.' }
        $before = (Get-FileHash -LiteralPath $source.FullName).Hash
        if ($displaySources.Contains($entry.Key)) {
            $expected=@($display.artifacts | Where-Object { $_.file -ceq $entry.Key })[0].sha256
            if ($before -ne $expected) { throw 'Display input changed after its manifest was verified.' }
        }
        if ($entry.Key -eq 'android/CapyIO-debug.apk' -and $before -ne $validatedApkHash) { throw 'APK changed during validation.' }
        $copied = Join-Path $destination $entry.Key
        New-Item -ItemType Directory -Path (Split-Path -Parent $copied) -Force | Out-Null
        Copy-Item -LiteralPath $source.FullName -Destination $copied
        if ((Get-FileHash -LiteralPath $copied).Hash -ne $before) { throw 'Artifact changed while packaging; stop concurrent builds.' }
        @{ file=$entry.Key; bytes=$source.Length; sha256=$before }
    }
    @{ schemaVersion=3; kind='capyio-local-debug-bundle'; productionRelease=$false; distribution='local-debug-only'; packagedAtUtc=[DateTime]::UtcNow.ToString('o'); sourceRevision=$(if ($UseExistingArtifacts) { $null } else { $revision }); packagingRevision=$revision; sourceTreeDirty=$dirty; buildProvenance=$(if ($UseExistingArtifacts) { 'existing-artifacts-source-not-verified' } else { 'first-party-rebuilt-third-party-explicit-stage' }); androidSignerSha256=$signers[0]; androidVersionCode=$androidVersionCode; androidVersionName=$androidVersionName; launch='capyio-desktop.exe'; displayLicenseNotice=$display.licenseNotice; artifacts=@($artifacts); limitations=@('Not a production installer or public distribution.', 'Existing drivers, camera registration, USB/IP/VIIPER and broker configuration remain external prerequisites.', 'Services/workers are included but not deployed or registered.', 'ADB is optional installation/diagnostic tooling, not node or media transport.') } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $destination 'manifest.json') -Encoding UTF8
    & (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $destination
    Write-Output "Local-only bundle: $destination"
    Write-Output "ONLY application entry: $(Join-Path $destination 'capyio-desktop.exe')"
} finally { $env:CAPYIO_BUILD_REVISION=$oldRevision; $env:CAPYIO_BUILD_TIME=$oldTime; Pop-Location }
