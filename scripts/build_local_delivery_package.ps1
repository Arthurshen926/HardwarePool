[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9A-Fa-f]{64}$')]
    [string]$ExpectedAndroidSignerSha256,
    [string]$AndroidDebugKeystore,
    [string]$AndroidSdkRoot = $env:ANDROID_SDK_ROOT,
    [switch]$UseExistingArtifacts,
    [switch]$SkipArchive
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$previousRevision = $env:CAPYIO_BUILD_REVISION
$previousBuildTime = $env:CAPYIO_BUILD_TIME
$previousSdkRoot = $env:ANDROID_SDK_ROOT
if (-not $AndroidSdkRoot) { $AndroidSdkRoot = $env:ANDROID_HOME }
if (-not $AndroidSdkRoot -and $env:LOCALAPPDATA) {
    $AndroidSdkRoot = Join-Path $env:LOCALAPPDATA 'Android\Sdk'
}
if (-not $AndroidSdkRoot -or -not (Test-Path -LiteralPath $AndroidSdkRoot -PathType Container)) {
    throw 'Supply -AndroidSdkRoot pointing to the Android SDK with aapt2 and apksigner.'
}
$buildTools = Get-ChildItem -LiteralPath (Join-Path $AndroidSdkRoot 'build-tools') -Directory |
    Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
    Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if ($null -eq $buildTools) { throw 'No stable Android SDK build-tools installation was found.' }
$apkSigner = Join-Path $buildTools.FullName 'apksigner.bat'
$aapt = Join-Path $buildTools.FullName 'aapt2.exe'
foreach ($tool in @($apkSigner, $aapt)) {
    if (-not (Test-Path -LiteralPath $tool -PathType Leaf)) { throw "Missing Android build tool: $tool" }
}
if (-not $UseExistingArtifacts) {
    if (-not $AndroidDebugKeystore -or -not (Test-Path -LiteralPath $AndroidDebugKeystore -PathType Leaf)) {
        throw 'Supply the existing -AndroidDebugKeystore. A different debug key cannot update an installed app without losing its data.'
    }
    $AndroidDebugKeystore = (Resolve-Path -LiteralPath $AndroidDebugKeystore).Path
}

Push-Location $repositoryRoot
try {
    $env:ANDROID_SDK_ROOT = (Resolve-Path -LiteralPath $AndroidSdkRoot).Path
    $revision = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve source revision.' }
    $dirty = [bool](& git status --porcelain --untracked-files=normal)
    $env:CAPYIO_BUILD_REVISION = $revision + $(if ($dirty) { '-dirty' } else { '' })
    $env:CAPYIO_BUILD_TIME = [DateTime]::UtcNow.ToString('o')
    if (-not $UseExistingArtifacts) {
        & pnpm --filter '@capyio/desktop' build
        if ($LASTEXITCODE -ne 0) { throw 'Shared UI build failed.' }
        & cargo build --offline -p capyio-desktop --bin capyio-desktop --features tauri/custom-protocol
        if ($LASTEXITCODE -ne 0) { throw 'Embedded Windows application build failed.' }
        & cargo build --offline -p capyio-remote-touchpad-adapter -p capyio-vcamdroid-adapter -p capyio-windows-camera-mf --bin capyio-ptp-adb-lab --bin capyio-avc-lab-receiver --bin capyio-camera-session-host
        if ($LASTEXITCODE -ne 0) { throw 'Windows helper build failed.' }
        Push-Location (Join-Path $repositoryRoot 'platform/android')
        try {
            # This entrypoint is deliberately debug-only. These are the standard
            # Android debug credentials, never a production signing interface.
            & .\gradlew.bat --offline --no-daemon :app:assembleDebug :app:verifyIntegratedDebugApk `
                "-Pandroid.injected.signing.store.file=$AndroidDebugKeystore" `
                '-Pandroid.injected.signing.store.password=android' `
                '-Pandroid.injected.signing.key.alias=androiddebugkey' `
                '-Pandroid.injected.signing.key.password=android'
            if ($LASTEXITCODE -ne 0) { throw 'Unified Android debug APK build failed.' }
        } finally { Pop-Location }
    }

    $apk = Join-Path $repositoryRoot 'platform/android/app/build/outputs/apk/debug/app-debug.apk'
    if (-not (Test-Path -LiteralPath $apk -PathType Leaf)) { throw 'Unified debug APK is missing.' }
    $validatedApkHash = (Get-FileHash -LiteralPath $apk -Algorithm SHA256).Hash
    $signerOutput = @(& $apkSigner verify --print-certs $apk 2>&1)
    if ($LASTEXITCODE -ne 0) { throw 'APK signature verification failed.' }
    $signerHashes = @($signerOutput | ForEach-Object {
        if ([string]$_ -match '^Signer #\d+ certificate SHA-256 digest: ([0-9a-fA-F]{64})$') { $Matches[1].ToUpperInvariant() }
    })
    if ($signerHashes.Count -ne 1 -or $signerHashes[0] -cne $ExpectedAndroidSignerSha256.ToUpperInvariant()) {
        throw 'APK signer does not match the expected installed-app identity. Preserve the installed app; rebuild with its original debug keystore. Do not uninstall or clear data.'
    }
    $badging = @(& $aapt dump badging $apk 2>&1)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read the Android package identity.' }
    $packageLine = @($badging | Where-Object { [string]$_ -match '^package: ' })
    if ($packageLine.Count -ne 1 -or [string]$packageLine[0] -notmatch "^package: name='dev\.capyio\.android' versionCode='(\d+)' versionName='([^']+)' ") {
        throw 'APK is not the unified dev.capyio.android package.'
    }
    $versionCode = [int]$Matches[1]
    $versionName = $Matches[2]
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($apk)
    try {
        foreach ($entryName in @('AndroidManifest.xml', 'classes.dex', 'assets/capyio-ui/index.html', 'lib/arm64-v8a/libcapyio_android_jni.so')) {
            if ($null -eq $archive.GetEntry($entryName)) { throw "Unified APK entry is missing: $entryName" }
        }
    } finally { $archive.Dispose() }

    $stamp = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-ffff')
    $destination = Join-Path $repositoryRoot "target\local-delivery\CapyIO-test-$stamp"
    if (Test-Path -LiteralPath $destination) { throw 'Refusing to overwrite an existing delivery directory.' }
    New-Item -ItemType Directory -Path (Join-Path $destination 'android') | Out-Null
    $sources = [ordered]@{
        'capyio-desktop.exe' = 'target/debug/capyio-desktop.exe'
        'capyio-ptp-adb-lab.exe' = 'target/debug/capyio-ptp-adb-lab.exe'
        'capyio-avc-lab-receiver.exe' = 'target/debug/capyio-avc-lab-receiver.exe'
        'capyio-camera-session-host.exe' = 'target/debug/capyio-camera-session-host.exe'
        'android/CapyIO-debug.apk' = 'platform/android/app/build/outputs/apk/debug/app-debug.apk'
        'README.md' = 'docs/LOCAL_TEST_DELIVERY.md'
        'LICENSE' = 'LICENSE'
        'verify_integrated_test_package.ps1' = 'scripts/verify_integrated_test_package.ps1'
    }
    $artifacts = foreach ($entry in $sources.GetEnumerator()) {
        $source = Join-Path $repositoryRoot $entry.Value
        $file = Get-Item -LiteralPath $source
        if ($file.PSIsContainer -or $file.Length -le 0 -or $file.Length -gt 536870912 -or
                ($file.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Invalid build artifact: $($entry.Key)"
        }
        $copied = Join-Path $destination $entry.Key
        Copy-Item -LiteralPath $source -Destination $copied
        $copiedHash = (Get-FileHash -LiteralPath $copied -Algorithm SHA256).Hash
        if ($entry.Key -eq 'android/CapyIO-debug.apk' -and $copiedHash -cne $validatedApkHash) {
            throw 'APK changed during signature validation/copy. Stop concurrent builds and package again.'
        }
        [ordered]@{ file = $entry.Key; bytes = $file.Length; sha256 = $copiedHash }
    }
    $manifest = [ordered]@{
        schemaVersion = 2
        kind = 'capyio-local-test-delivery'
        productionRelease = $false
        packagedAtUtc = [DateTime]::UtcNow.ToString('o')
        packagingRevision = $revision
        sourceRevision = $(if ($UseExistingArtifacts) { $null } else { $revision })
        sourceTreeDirty = $dirty
        buildProvenance = $(if ($UseExistingArtifacts) { 'existing-artifacts-source-not-verified' } else { 'rebuilt-debug-embedded-ui' })
        android = [ordered]@{ applicationId = 'dev.capyio.android'; versionCode = $versionCode; versionName = $versionName; abi = 'arm64-v8a'; signerSha256 = $signerHashes[0]; signing = 'debug' }
        artifacts = @($artifacts)
        limitations = @('Local testing only; no public release or production signature.', 'No separately deployed drivers, services, external tools, keys, invitations or user configuration are included.', 'System projections require their separately deployed components.', 'Public redistribution still requires a complete dependency license/notice review.', 'Package verification is not a hardware acceptance result or an authenticated publisher signature.')
    }
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $destination 'manifest.json') -Encoding UTF8
    & (Join-Path $PSScriptRoot 'verify_integrated_test_package.ps1') -PackageRoot $destination
    if (-not $SkipArchive) {
        $zipPath = "$destination.zip"
        [IO.Compression.ZipFile]::CreateFromDirectory($destination, $zipPath, [IO.Compression.CompressionLevel]::Optimal, $true)
        Write-Output "Archive: $zipPath"
        Write-Output "Archive SHA-256: $((Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash)"
    }
    Write-Output "Local test delivery: $destination"
    Write-Output "Launch: $(Join-Path $destination 'capyio-desktop.exe')"
} finally {
    $env:CAPYIO_BUILD_REVISION = $previousRevision
    $env:CAPYIO_BUILD_TIME = $previousBuildTime
    $env:ANDROID_SDK_ROOT = $previousSdkRoot
    Pop-Location
}
