[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\capyio-ptp-adb-lab.exe'
$expectedHash = '65C96C37EB14513E08C55116EA9B52DFB18AE21B7E137218E90BD2652B6C820B'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003X-android-vhf-visible-cursor.txt'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The visible-cursor Android-to-VHF acceptance requires elevated PowerShell.'
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
    Write-Output 'acceptance=CAPY-PTP-003X'
    Write-Output 'projection=android-adb-reverse-to-installed-vhf'
    Write-Output 'cursor_precondition=003W-one-pixel-relative-mouse-wake'
    Write-Output 'android_apk_version=0.9'
    Write-Output "executable_sha256=$actualHash"
    Write-Output 'restart_authorized=false'
    Write-Output 'driver_or_apk_installation_performed=false'
    $labOutput = @(& $executable `
        --inject `
        --acknowledge-desktop-input `
        --vhf `
        --manual-session 2>&1)
    $labExit = $LASTEXITCODE
    $labOutput | Write-Output
    if ($labExit -ne 0) {
        throw "Visible-cursor Android-to-VHF acceptance failed: $labExit"
    }
    $labText = $labOutput -join [Environment]::NewLine
    if ($labText -notmatch '(?m)^max_contacts_observed=([1-5])\s*$') {
        throw 'Visible-cursor run did not observe physical touch input.'
    }
    Write-Output 'CAPY-PTP-003X visible-cursor live comparison: PASS'
}
finally {
    Stop-Transcript
}
