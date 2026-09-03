[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\capyio-ptp-adb-lab.exe'
$expectedHash = 'D7C49E4111E71F1541F08485F334485851E574B29C4B7C8C6392422DA50233D3'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-0041-android-vhf-tap-drag-trace.txt'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Android/VHF tap-and-drag tracing requires elevated PowerShell.'
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
    Write-Output 'acceptance=CAPY-PTP-0041-android-trace'
    Write-Output 'projection=android-adb-reverse-to-installed-vhf'
    Write-Output 'trace=two-complete-one-contact-gestures'
    Write-Output 'android_apk_version=1.2'
    Write-Output 'android_apk_sha256=DC4D5015CC04074A003E8150EACB4D712B4BEA6447800E7377F06F4967B2F707'
    Write-Output "executable_sha256=$actualHash"
    Write-Output 'driver_or_apk_installation_performed=false'
    Write-Output 'restart_authorized=false'
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $labOutput = @(& $executable `
            --inject `
            --acknowledge-desktop-input `
            --vhf `
            --trace-tap-drag 2>&1)
        $labExit = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    $labOutput | Write-Output
    if ($labExit -ne 0) {
        throw "Android/VHF tap-and-drag trace failed: $labExit"
    }
    $labText = $labOutput -join [Environment]::NewLine
    if ($labText -notmatch '(?m)^tap_drag_trace_complete=true\s*$') {
        throw 'Tap-and-drag trace did not capture two complete one-contact gestures.'
    }
    if ($labText -notmatch '(?m)^max_contacts_observed=1\s*$') {
        throw 'Tap-and-drag trace included a non-single-contact gesture.'
    }
    Write-Output 'CAPY-PTP-0041 Android/VHF tap-and-drag trace: PASS'
}
finally {
    Stop-Transcript
}
