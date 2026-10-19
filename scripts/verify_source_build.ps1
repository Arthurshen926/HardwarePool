[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$CandidateDirectory,
    [Parameter(Mandatory)][string]$AndroidDebugKeystore,
    [string]$AndroidSdkRoot=$env:ANDROID_HOME,
    [string]$AndroidBuildToolsVersion='36.0.0',
    [string]$JavaHome=(Join-Path $env:ProgramFiles 'Android/Android Studio/jbr'),
    [switch]$KeepBuildCache
)
# Build a disposable copy; never install packages on devices or change source candidate.
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent $PSScriptRoot
$candidate=(Get-Item -LiteralPath $CandidateDirectory).FullName
& python (Join-Path $PSScriptRoot 'export_source_candidate.py') --verify $candidate
if($LASTEXITCODE){throw 'Source candidate integrity failed'}
foreach($path in @($AndroidDebugKeystore,(Join-Path $JavaHome 'bin/java.exe'))) {
    if(-not (Test-Path -LiteralPath $path -PathType Leaf)){throw "Missing build prerequisite: $path"}
}
if(-not (Test-Path -LiteralPath $AndroidSdkRoot -PathType Container)){throw 'Android SDK directory missing'}
# Resolve caller-relative paths before switching into the disposable source tree.
$AndroidDebugKeystore=(Get-Item -LiteralPath $AndroidDebugKeystore).FullName
$AndroidSdkRoot=(Get-Item -LiteralPath $AndroidSdkRoot).FullName
$JavaHome=(Get-Item -LiteralPath $JavaHome).FullName
$apkSigner=Join-Path $AndroidSdkRoot ('build-tools/'+$AndroidBuildToolsVersion+'/apksigner.bat')
if(-not (Test-Path -LiteralPath $apkSigner -PathType Leaf)){throw "Missing build prerequisite: $apkSigner"}
$drive=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($repo))
if($drive.AvailableFreeSpace -lt 10GB){throw 'At least 10 GiB free space is required before an independent build; clean old build caches first.'}
$run=Join-Path $repo ('target/source-build/'+[DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-ffff'))
$source=Join-Path $run 'src'
New-Item -ItemType Directory -Path $source|Out-Null
$manifest=Get-Content -LiteralPath (Join-Path $candidate 'manifest.json') -Raw|ConvertFrom-Json
foreach($entry in $manifest.artifacts) {
    $destination=Join-Path $source $entry.file
    New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force|Out-Null
    Copy-Item -LiteralPath (Join-Path (Join-Path $candidate 'candidate') $entry.file) -Destination $destination
}
$state=@{schemaVersion=1;sourceManifestSha256=(Get-FileHash (Join-Path $candidate 'manifest.json')).Hash;publicationApproved=$false;steps=@();artifacts=@();buildMode='fresh-first-party-offline-dependency-cache';devDebugSymbols=$false}
function Save-State { $state|ConvertTo-Json -Depth 8|Set-Content (Join-Path $run 'result.json') }
function Run-Step($name,[scriptblock]$action) {
    & $action *> (Join-Path $run "$name.log")
    if($LASTEXITCODE -ne 0){$state.steps+=@{name=$name;passed=$false};Save-State;throw "Build step failed: $name; evidence: $run"}
    $state.steps+=@{name=$name;passed=$true};Save-State
    Write-Output "passed=$name"
}
$variables=@('JAVA_HOME','ANDROID_HOME','ANDROID_SDK_ROOT','CARGO_TARGET_DIR','CARGO_PROFILE_DEV_DEBUG','CAPYIO_BUILD_REVISION','CARGO_INCREMENTAL')
$saved=@{}
foreach($name in $variables){$saved[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
try {
    $env:JAVA_HOME=$JavaHome
    $env:ANDROID_HOME=$AndroidSdkRoot
    $env:ANDROID_SDK_ROOT=$AndroidSdkRoot
    $env:CARGO_TARGET_DIR=Join-Path $run 'cargo-target'
    $env:CARGO_PROFILE_DEV_DEBUG='0'
    $env:CARGO_INCREMENTAL='0'
    $env:CAPYIO_BUILD_REVISION='source-manifest-'+$state.sourceManifestSha256.Substring(0,12)
    Push-Location $source
    try {
        Run-Step 'dependencies' { pnpm install --offline --frozen-lockfile }
        Run-Step 'frontend' { pnpm --filter '@capyio/desktop' build }
        Run-Step 'desktop' { cargo build --offline --locked -p capyio-desktop --bin capyio-desktop --features tauri/custom-protocol }
        Run-Step 'helpers' { cargo build --offline --locked -p capyio-windows-service -p capyio-native-audio-lan -p capyio-remote-touchpad-adapter -p capyio-vcamdroid-adapter -p capyio-windows-camera-mf --bins }
        Run-Step 'android' {
            & ./platform/android/gradlew.bat -p platform/android :app:verifyIntegratedDebugApk --offline --no-build-cache --console=plain `
                "-Pandroid.injected.signing.store.file=$AndroidDebugKeystore" '-Pandroid.injected.signing.store.password=android' `
                '-Pandroid.injected.signing.key.alias=androiddebugkey' '-Pandroid.injected.signing.key.password=android'
        }
        $apk=Join-Path $source 'platform/android/app/build/outputs/apk/debug/app-debug.apk'
        Run-Step 'android-signature' { & $apkSigner verify --print-certs $apk }
        $signature=Get-Content (Join-Path $run 'android-signature.log') -Raw
        $match=[regex]::Match($signature,'Signer #1 certificate SHA-256 digest: ([a-fA-F0-9]{64})')
        if(-not $match.Success){throw 'Verified APK signer fingerprint was not returned'}
        $state.androidSignerSha256=$match.Groups[1].Value.ToLowerInvariant()
        $names=@('capyio-desktop','capyio-windows-service','capyio-native-virtual-microphone','capyio-native-virtual-speaker','capyio-ptp-adb-lab','capyio-avc-lab-receiver','capyio-camera-session-host','capyio-camera-virtual-lab')
        # Keep deliverables outside the disposable compiler cache.
        $artifactRoot=Join-Path $run 'artifacts'
        New-Item -ItemType Directory -Path (Join-Path $artifactRoot 'windows'),(Join-Path $artifactRoot 'android')|Out-Null
        foreach($name in $names) {
            Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR ('debug/'+$name+'.exe')) -Destination (Join-Path $artifactRoot ('windows/'+$name+'.exe'))
        }
        Copy-Item -LiteralPath $apk -Destination (Join-Path $artifactRoot 'android/CapyIO-debug.apk')
        $outputs=@($names|ForEach-Object {Join-Path $artifactRoot ('windows/'+$_+'.exe')})
        $outputs+=Join-Path $artifactRoot 'android/CapyIO-debug.apk'
        foreach($path in $outputs) {
            $state.artifacts+=@{file=[IO.Path]::GetRelativePath($run,$path);bytes=(Get-Item $path).Length;sha256=(Get-FileHash $path).Hash}
        }
        $state.artifactLayout='retained-v1'
        foreach($entry in $manifest.artifacts) {
            if((Get-FileHash (Join-Path $source $entry.file)).Hash -ne $entry.sha256){throw 'Build changed an exported source input'}
        }
        $state.sourceInputsUnchanged=$true
        Save-State
    } finally {Pop-Location}
} finally {
    foreach($name in $variables){[Environment]::SetEnvironmentVariable($name,$saved[$name],'Process')}
}
if(-not $KeepBuildCache) {
    $cache=(Resolve-Path -LiteralPath (Join-Path $run 'cargo-target')).Path
    $expected=[IO.Path]::GetFullPath((Join-Path $run 'cargo-target'))
    if($cache -ne $expected -or -not $cache.StartsWith([IO.Path]::GetFullPath($run)+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Unexpected build-cache location' }
    if((Get-Item -LiteralPath $cache).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Build cache must not be a reparse point' }
    Remove-Item -LiteralPath $cache -Recurse -Force -ErrorAction Stop
    $state.buildCacheRetained=$false
} else { $state.buildCacheRetained=$true }
Save-State
Write-Output "source_build=$run"
Write-Output "artifacts=$artifactRoot"
