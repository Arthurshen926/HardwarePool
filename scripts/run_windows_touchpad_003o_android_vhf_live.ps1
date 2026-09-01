[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\capyio-ptp-adb-lab.exe'
$expectedHash = '3AB868F22805815D3E45AF9D05340823030F6FC3D1A09C9F2F805E9E0532DA69'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003O-android-vhf-live.txt'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The Android-to-VHF live acceptance requires an elevated administrator PowerShell.'
}
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "Exact Android lab executable is absent: $executable"
}
$actualHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash
if ($actualHash -ne $expectedHash) {
    throw "Android lab executable hash mismatch: $actualHash"
}

New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Start-Transcript -LiteralPath $evidencePath -Force
try {
    Write-Output 'acceptance=CAPY-PTP-003O'
    Write-Output 'projection=android-adb-reverse-to-installed-vhf'
    Write-Output "executable_sha256=$actualHash"
    Write-Output 'restart_authorized=false'
    Write-Output 'driver_or_apk_installation_performed=false'
    & $executable `
        --inject `
        --acknowledge-desktop-input `
        --vhf `
        --exit-after-release `
        --manual-session
    $labExit = $LASTEXITCODE
    if ($labExit -ne 0) {
        throw "Android-to-VHF live acceptance failed: $labExit"
    }
    Write-Output 'CAPY-PTP-003O Android-to-VHF first physical gesture: PASS'
}
finally {
    Stop-Transcript
}
