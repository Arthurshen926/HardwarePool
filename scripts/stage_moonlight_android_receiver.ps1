[CmdletBinding()]
param(
    [string]$StageRoot = "",
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

$downloads = Join-Path $StageRoot "downloads"
$sources = Join-Path $StageRoot "source"
$scratch = Join-Path $StageRoot "extract"
New-Item -ItemType Directory -Force -Path $downloads | Out-Null
New-Item -ItemType Directory -Force -Path $sources | Out-Null
New-Item -ItemType Directory -Force -Path $scratch | Out-Null

$archives = @(
    @{
        Name = "moonlight-android.zip"
        Url = "https://codeload.github.com/moonlight-stream/moonlight-android/zip/b48494cb96bff23d8886c4775cc4f39a1075495d"
        Sha256 = "1e79b4bf448d01dde9e78933bddc4d6ec2dc913c61055eb8df7d08ff7311f450"
    },
    @{
        Name = "moonlight-common-c.zip"
        Url = "https://codeload.github.com/moonlight-stream/moonlight-common-c/zip/874ac9548f1bd6f095ef2b435c42cdde460e7821"
        Sha256 = "6e19484fd51d504a6b5ad530d83e30d23aaac277d2a5b856b8445ae3a72020a7"
    },
    @{
        Name = "enet.zip"
        Url = "https://codeload.github.com/cgutman/enet/zip/aca87840b57f045a1f7f9299e4b1b9b8e2a5e2f1"
        Sha256 = "c84d3ac2678484a4b59488866e8731fa28f768daa5c2e0902e0891f8cd4bcbf1"
    },
    @{
        Name = "nanors.zip"
        Url = "https://codeload.github.com/sleepybishop/nanors/zip/b1e3c22ca0cdc0bb83e3cd6ed1a2fc77869ed99a"
        Sha256 = "d14dc457db6268590d42ebe059a34ef57e35bef2ae8b5e415f8561e3e1fce12d"
    }
)

foreach ($archive in $archives) {
    $path = Join-Path $downloads $archive.Name
    if (-not (Test-Path -LiteralPath $path)) {
        if ($Offline) {
            throw "offline archive missing: $path"
        }
        Invoke-WebRequest -Uri $archive.Url -OutFile $path
    }
    $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $archive.Sha256) {
        throw "archive hash mismatch for $($archive.Name): $actual"
    }
}

$patchPath = Join-Path $repoRoot "third_party\patches\moonlight-common-c-874ac954\0001-capyio-video-only-client.patch"
$patchHash = (Get-FileHash -LiteralPath $patchPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($patchHash -ne "07badc78a5e6e0ec58627271b1fe4b55ef6dac7de1f41c39642e7a6e48191ff2") {
    throw "Moonlight Common C patch hash mismatch: $patchHash"
}

$commonTarget = Join-Path $sources "moonlight-common-c"
$opensslTarget = Join-Path $sources "openssl"
$readyMarker = Join-Path $sources ".capyio-ready"
$expectedMarker = @(
    "moonlight-common-c=874ac9548f1bd6f095ef2b435c42cdde460e7821"
    "patch=$patchHash"
    "moonlight-android=b48494cb96bff23d8886c4775cc4f39a1075495d"
)
if ((Test-Path -LiteralPath $readyMarker) -and
        ((Get-Content -LiteralPath $readyMarker) -join "`n") -eq ($expectedMarker -join "`n")) {
    Write-Output $sources
    exit 0
}

foreach ($target in @($commonTarget, $opensslTarget)) {
    if (Test-Path -LiteralPath $target) {
        throw "Incomplete or changed stage exists; choose a new StageRoot: $target"
    }
}
if (@(Get-ChildItem -LiteralPath $scratch -Force).Count -ne 0) {
    throw "Extraction directory is not empty; choose a new StageRoot: $scratch"
}

foreach ($archive in $archives) {
    $destination = Join-Path $scratch ([IO.Path]::GetFileNameWithoutExtension($archive.Name))
    Expand-Archive -LiteralPath (Join-Path $downloads $archive.Name) -DestinationPath $destination -Force
}

$commonSource = Get-ChildItem (Join-Path $scratch "moonlight-common-c") -Directory | Select-Object -First 1
$androidSource = Get-ChildItem (Join-Path $scratch "moonlight-android") -Directory | Select-Object -First 1
$enetSource = Get-ChildItem (Join-Path $scratch "enet") -Directory | Select-Object -First 1
$nanorsSource = Get-ChildItem (Join-Path $scratch "nanors") -Directory | Select-Object -First 1
if ($null -in @($commonSource, $androidSource, $enetSource, $nanorsSource)) {
    throw "one or more fixed archives have an unexpected root layout"
}

Copy-Item -LiteralPath $commonSource.FullName -Destination $commonTarget -Recurse
$enetTarget = Join-Path $commonTarget "enet"
$nanorsTarget = Join-Path $commonTarget "nanors"
New-Item -ItemType Directory -Force -Path $enetTarget | Out-Null
New-Item -ItemType Directory -Force -Path $nanorsTarget | Out-Null
Get-ChildItem -LiteralPath $enetSource.FullName -Force |
    Copy-Item -Destination $enetTarget -Recurse
Get-ChildItem -LiteralPath $nanorsSource.FullName -Force |
    Copy-Item -Destination $nanorsTarget -Recurse
$referenceOpenSsl = Join-Path $androidSource.FullName "app\src\main\jni\moonlight-core\openssl"
Copy-Item -LiteralPath $referenceOpenSsl -Destination $opensslTarget -Recurse

$commonRelative = [IO.Path]::GetRelativePath($repoRoot, $commonTarget).Replace('\', '/')
& git -C $repoRoot apply --check "--directory=$commonRelative" $patchPath
if ($LASTEXITCODE -ne 0) {
    throw "CapyIO video-only patch does not apply to the pinned source"
}
& git -C $repoRoot apply "--directory=$commonRelative" $patchPath
if ($LASTEXITCODE -ne 0) {
    throw "CapyIO video-only patch failed"
}

$forbidden = @("AudioStream.c", "RtpAudioQueue.c", "InputStream.c")
$sourceList = Get-ChildItem (Join-Path $commonTarget "src") -File | Select-Object -ExpandProperty Name
foreach ($name in $forbidden) {
    if ($name -notin $sourceList) {
        throw "pinned source unexpectedly lacks audited file $name"
    }
}

Set-Content -LiteralPath $readyMarker -Value $expectedMarker -Encoding utf8NoBOM
Write-Output $sources
