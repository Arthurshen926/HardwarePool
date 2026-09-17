[CmdletBinding()]
param(
    [string]$StageRoot = "",
    [string]$AndroidSdkRoot = "",
    [string]$NdkVersion = "29.0.14206865",
    [string]$CMakePath = "",
    [string]$NinjaPath = "",
    [string[]]$Abis = @("arm64-v8a"),
    [switch]$Offline
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($StageRoot)) {
    $StageRoot = Join-Path $repoRoot "target\android-moonlight-receiver"
}
$StageRoot = [IO.Path]::GetFullPath($StageRoot)
$allowedRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot "target"))
if (-not $StageRoot.StartsWith($allowedRoot + [IO.Path]::DirectorySeparatorChar,
        [StringComparison]::OrdinalIgnoreCase)) {
    throw "StageRoot must remain below the repository target directory"
}

if ([string]::IsNullOrWhiteSpace($AndroidSdkRoot)) {
    if (-not [string]::IsNullOrWhiteSpace($env:ANDROID_SDK_ROOT)) {
        $AndroidSdkRoot = $env:ANDROID_SDK_ROOT
    } else {
        $AndroidSdkRoot = Join-Path $env:LOCALAPPDATA "Android\Sdk"
    }
}
$ndkRoot = Join-Path ([IO.Path]::GetFullPath($AndroidSdkRoot)) "ndk\$NdkVersion"
$toolchain = Join-Path $ndkRoot "build\cmake\android.toolchain.cmake"
if (-not (Test-Path -LiteralPath $toolchain)) {
    throw "Android NDK toolchain is missing: $toolchain"
}

function Resolve-BuildTool([string]$Requested, [string]$Name) {
    if (-not [string]::IsNullOrWhiteSpace($Requested)) {
        $resolved = (Get-Command $Requested -ErrorAction Stop).Source
        return [IO.Path]::GetFullPath($resolved)
    }
    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return [IO.Path]::GetFullPath($command.Source)
    }
    $auditMatches = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot "target\upstream-audit") `
        -Recurse -File -Filter "$Name.exe" -ErrorAction SilentlyContinue |
        Sort-Object FullName)
    if ($auditMatches.Count -eq 1) {
        return $auditMatches[0].FullName
    }
    throw "pass -${Name}Path with an exact executable path"
}

$CMakePath = Resolve-BuildTool $CMakePath "cmake"
$NinjaPath = Resolve-BuildTool $NinjaPath "ninja"
$sourceRoot = & (Join-Path $PSScriptRoot "stage_moonlight_android_receiver.ps1") `
    -StageRoot $StageRoot -Offline:$Offline
if ($LASTEXITCODE -ne 0) {
    throw "Moonlight receiver source staging failed"
}
$sourceRoot = [IO.Path]::GetFullPath(($sourceRoot | Select-Object -Last 1))
$jniSource = Join-Path $repoRoot "platform\android\capyio-display-android\display-app\src\main\jni"
$jniLibRoot = Join-Path $StageRoot "jniLibs"
New-Item -ItemType Directory -Force -Path $jniLibRoot | Out-Null

$supportedAbis = @("arm64-v8a", "armeabi-v7a", "x86", "x86_64")
$artifacts = @()
foreach ($abi in $Abis) {
    if ($abi -notin $supportedAbis) {
        throw "unsupported Android ABI: $abi"
    }
    $buildRoot = Join-Path $StageRoot "native-build\$abi"
    $abiOutput = Join-Path $jniLibRoot $abi
    New-Item -ItemType Directory -Force -Path $buildRoot | Out-Null
    New-Item -ItemType Directory -Force -Path $abiOutput | Out-Null

    & $CMakePath -S $jniSource -B $buildRoot -G Ninja `
        "-DCMAKE_MAKE_PROGRAM=$NinjaPath" `
        "-DCMAKE_TOOLCHAIN_FILE=$toolchain" `
        "-DCMAKE_BUILD_TYPE=Release" `
        "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON" `
        "-DANDROID_ABI=$abi" `
        "-DANDROID_PLATFORM=android-29" `
        "-DCAPYIO_MOONLIGHT_ROOT=$sourceRoot"
    if ($LASTEXITCODE -ne 0) {
        throw "CMake configure failed for $abi"
    }
    & $CMakePath --build $buildRoot
    if ($LASTEXITCODE -ne 0) {
        throw "native build failed for $abi"
    }
    $library = Join-Path $buildRoot "libcapyio-gamestream-client.so"
    if (-not (Test-Path -LiteralPath $library)) {
        throw "native build did not produce $library"
    }
    Copy-Item -LiteralPath $library -Destination (Join-Path $abiOutput ([IO.Path]::GetFileName($library))) -Force
    $artifacts += @{
        abi = $abi
        sha256 = (Get-FileHash -LiteralPath $library -Algorithm SHA256).Hash.ToLowerInvariant()
        compileCommandsSha256 = (Get-FileHash -LiteralPath (Join-Path $buildRoot 'compile_commands.json') -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

$sourceFiles = @(Get-ChildItem -LiteralPath $sourceRoot -File -Recurse | Sort-Object FullName | ForEach-Object {
    @{ path = [IO.Path]::GetRelativePath($sourceRoot, $_.FullName).Replace('\', '/'); sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
})
@{
    schemaVersion = 1
    distribution = 'local-debug-only'
    ndkVersion = $NdkVersion
    patchSha256 = (Get-FileHash -LiteralPath (Join-Path $repoRoot 'third_party/patches/moonlight-common-c-874ac954/0001-capyio-video-only-client.patch') -Algorithm SHA256).Hash.ToLowerInvariant()
    artifacts = $artifacts
    sourceFiles = $sourceFiles
    integrationFiles = @(Get-ChildItem -LiteralPath $jniSource -File | Sort-Object FullName | ForEach-Object {
        @{ path = $_.Name; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    })
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $StageRoot 'build-manifest.json') -Encoding utf8NoBOM

Write-Output $jniLibRoot
