[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\capyio-ptp-adb-lab.exe'
$expectedHash = '65C96C37EB14513E08C55116EA9B52DFB18AE21B7E137218E90BD2652B6C820B'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003U-android-vhf-oem-gestures-disabled-attempt2.txt'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The OEM-gesture-disabled Android-to-VHF acceptance requires elevated PowerShell.'
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
    Write-Output 'acceptance=CAPY-PTP-003U'
    Write-Output 'attempt=2'
    Write-Output 'projection=android-adb-reverse-to-installed-vhf'
    Write-Output 'android_apk_version=0.8'
    Write-Output 'oem_three_finger_gestures=disabled-by-user'
    Write-Output 'diagnostic_policy=probable-system-interception-counter'
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
        throw "OEM-gesture-disabled Android-to-VHF acceptance failed: $labExit"
    }
    $labText = $labOutput -join [Environment]::NewLine
    $maxMatch = [regex]::Match($labText, '(?m)^max_contacts_observed=([0-9]+)\s*$')
    if (-not $maxMatch.Success) {
        throw 'OEM-gesture-disabled run did not report max_contacts_observed.'
    }
    $maxContacts = [int]$maxMatch.Groups[1].Value
    if ($maxContacts -lt 4) {
        throw "OEM-gesture-disabled run observed only $maxContacts contacts; four are required."
    }
    Write-Output (
        'CAPY-PTP-003U OEM-gesture-disabled Android-to-VHF multitouch: ' +
            "PASS; max_contacts=$maxContacts"
    )
}
finally {
    Stop-Transcript
}
