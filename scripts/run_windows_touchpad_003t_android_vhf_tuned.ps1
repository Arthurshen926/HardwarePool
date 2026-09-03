[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\capyio-ptp-adb-lab.exe'
$expectedHash = '65C96C37EB14513E08C55116EA9B52DFB18AE21B7E137218E90BD2652B6C820B'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003T-android-vhf-tuned.txt'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The tuned Android-to-VHF acceptance requires an elevated administrator PowerShell.'
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
    Write-Output 'acceptance=CAPY-PTP-003T'
    Write-Output 'projection=android-adb-reverse-to-installed-vhf'
    Write-Output 'android_capture_policy=three-plus-continuous-anchor-scale-700-per-mille'
    Write-Output 'android_move_settle=initial-24ms-added-contact-72ms'
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
        throw "Tuned Android-to-VHF acceptance failed: $labExit"
    }
    $labText = $labOutput -join [Environment]::NewLine
    $maxMatch = [regex]::Match($labText, '(?m)^max_contacts_observed=([0-9]+)\s*$')
    if (-not $maxMatch.Success) {
        throw 'Tuned Android-to-VHF output did not report max_contacts_observed.'
    }
    $maxContacts = [int]$maxMatch.Groups[1].Value
    if ($maxContacts -lt 4) {
        throw "Tuned Android-to-VHF run observed only $maxContacts contacts; four are required."
    }
    Write-Output "CAPY-PTP-003T tuned Android-to-VHF multitouch: PASS; max_contacts=$maxContacts"
}
finally {
    Stop-Transcript
}
