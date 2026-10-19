[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path $repositoryRoot ('target\package-tests\' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $testRoot 'android') | Out-Null
$verify = Join-Path $PSScriptRoot 'verify_integrated_test_package.ps1'
$fileNames = @('capyio-desktop.exe', 'capyio-ptp-adb-lab.exe', 'capyio-avc-lab-receiver.exe',
    'capyio-camera-session-host.exe', 'android/CapyIO-debug.apk', 'README.md', 'LICENSE',
    'verify_integrated_test_package.ps1')
foreach ($name in $fileNames) {
    [IO.File]::WriteAllBytes((Join-Path $testRoot $name), [byte[]]@(1, 2, 3))
}
$artifacts = foreach ($name in $fileNames) {
    @{ file = $name; bytes = 3; sha256 = (Get-FileHash -LiteralPath (Join-Path $testRoot $name)).Hash }
}
$manifest = @{
    schemaVersion = 2; kind = 'capyio-local-test-delivery'; productionRelease = $false
    buildProvenance = 'existing-artifacts-source-not-verified'
    android = @{ signerSha256 = ('A' * 64) }; artifacts = @($artifacts)
}
$manifestPath = Join-Path $testRoot 'manifest.json'
function Save-Manifest { $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifestPath -Encoding UTF8 }
function Expect-Rejection([string]$Name, [string]$ExpectedMessage) {
    $rejected = $false
    try { & $verify -PackageRoot $testRoot | Out-Null } catch {
        if ($_.Exception.Message -notlike $ExpectedMessage) { throw "${Name}: unexpected error: $($_.Exception.Message)" }
        $rejected = $true
    }
    if (-not $rejected) { throw "${Name}: invalid package was accepted" }
    Write-Output "PASS: $Name"
}
Save-Manifest
& $verify -PackageRoot $testRoot | Out-Null
Write-Output 'PASS: complete synthetic package'

$changedFile = Join-Path $testRoot 'capyio-desktop.exe'
[IO.File]::WriteAllBytes($changedFile, [byte[]]@(3, 2, 1))
Expect-Rejection 'same-size tampering' 'Package checksum mismatch:*'
[IO.File]::WriteAllBytes($changedFile, [byte[]]@(1, 2, 3))

$manifest.artifacts[0].file = '../outside.exe'
Save-Manifest
Expect-Rejection 'path traversal' 'Unexpected or duplicate package entry:*'
$manifest.artifacts[0].file = 'capyio-desktop.exe'
$manifest.artifacts[1].file = 'capyio-desktop.exe'
Save-Manifest
Expect-Rejection 'duplicate file entry' 'Unexpected or duplicate package entry:*'
$manifest.artifacts[1].file = 'capyio-ptp-adb-lab.exe'

$manifest.artifacts = @($artifacts | Select-Object -Skip 1)
Save-Manifest
Expect-Rejection 'incomplete manifest' 'Unsupported or incomplete*'
$manifest.artifacts = @($artifacts)
$manifest.productionRelease = $true
Save-Manifest
Expect-Rejection 'false production release claim' 'Unsupported or incomplete*'
$manifest.productionRelease = $false
Save-Manifest

[IO.File]::WriteAllBytes((Join-Path $testRoot 'accidental-invitation.cpi'), [byte[]]@(1))
Expect-Rejection 'unlisted private file' 'Unexpected package file:*'
# Keep this unique ignored fixture directory for diagnostic evidence. No user
# files are deleted and no executables or APKs are run by this test.
Write-Output 'All 6 package rejection cases and the complete-package case passed.'
Write-Output "Synthetic evidence: $testRoot"
