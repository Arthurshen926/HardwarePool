[CmdletBinding()]
param(
    [string]$GameStreamStageRoot = "target/android-moonlight-unified",
    [string]$JavaHome = "",
    [string]$BuildRevision = "integrated-debug",
    [switch]$Online
)

# Complete local-lab debug package. Never installs an APK, rebuilds third-party
# sources implicitly, or enables the GPL display integration for Release.
$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$stageRoot = if ([IO.Path]::IsPathRooted($GameStreamStageRoot)) {
    [IO.Path]::GetFullPath($GameStreamStageRoot)
} else {
    [IO.Path]::GetFullPath((Join-Path $repoRoot $GameStreamStageRoot))
}
$allowedRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot "target"))
if (-not $stageRoot.StartsWith($allowedRoot + [IO.Path]::DirectorySeparatorChar,
        [StringComparison]::OrdinalIgnoreCase)) {
    throw "GameStreamStageRoot must remain below repository target/"
}
foreach ($inputPath in @("build-manifest.json", "jniLibs/arm64-v8a/libcapyio-gamestream-client.so")) {
    if (-not (Test-Path -LiteralPath (Join-Path $stageRoot $inputPath) -PathType Leaf)) {
        throw "Missing verified GameStream stage input: $inputPath. Run scripts/build_moonlight_android_receiver.ps1 first."
    }
}
if ([string]::IsNullOrWhiteSpace($JavaHome)) {
    $JavaHome = $env:JAVA_HOME
}
if ([string]::IsNullOrWhiteSpace($JavaHome)) {
    $JavaHome = Join-Path $env:ProgramFiles "Android/Android Studio/jbr"
}
if (-not (Test-Path -LiteralPath (Join-Path $JavaHome "bin/java.exe") -PathType Leaf)) {
    throw "A Java runtime is required; supply -JavaHome or JAVA_HOME."
}
$stageRelative = [IO.Path]::GetRelativePath($repoRoot, $stageRoot).Replace('\', '/')
$previousJava = $env:JAVA_HOME
$previousRevision = $env:CAPYIO_BUILD_REVISION
try {
    $env:JAVA_HOME = $JavaHome
    $env:CAPYIO_BUILD_REVISION = $BuildRevision
    $arguments = @("-p", (Join-Path $repoRoot "platform/android"),
        ":app:verifyIntegratedDebugApk", ":app:verifyGameStreamDebugApk",
        "-PcapyioGameStreamStageRoot=$stageRelative", "--console=plain")
    if (-not $Online) { $arguments += "--offline" }
    & (Join-Path $repoRoot "platform/android/gradlew.bat") @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Integrated Android debug build or artifact verification failed (exit $LASTEXITCODE)."
    }
    $apk = Get-Item -LiteralPath (Join-Path $repoRoot "platform/android/app/build/outputs/apk/debug/app-debug.apk")
    [pscustomobject]@{
        Path = $apk.FullName
        Bytes = $apk.Length
        Sha256 = (Get-FileHash -LiteralPath $apk.FullName -Algorithm SHA256).Hash
        DisplayNativeVerified = $true
        Distribution = "local-debug-only"
    }
} finally {
    $env:JAVA_HOME = $previousJava
    $env:CAPYIO_BUILD_REVISION = $previousRevision
}
