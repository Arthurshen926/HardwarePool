[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\capyio-ptp-adb-lab.exe'
$expectedHash = '65C96C37EB14513E08C55116EA9B52DFB18AE21B7E137218E90BD2652B6C820B'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003V-android-vhf-cursor-attempt8.txt'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The live Android VHF cursor acceptance requires elevated PowerShell.'
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
    Write-Output 'acceptance=CAPY-PTP-003V'
    Write-Output 'attempt=8'
    Write-Output 'projection=android-adb-reverse-to-installed-vhf'
    Write-Output 'gesture=exactly-one-contact-motion'
    Write-Output 'cursor_probe=anchor-virtual-desktop-center-and-observe-delta'
    Write-Output 'android_lab_version=1.0'
    Write-Output 'capyio_must_remain_foreground=true'
    Write-Output 'uu_remote_must_remain_background=true'
    Write-Output "executable_sha256=$actualHash"
    Write-Output 'restart_authorized=false'
    Write-Output 'driver_or_apk_installation_performed=false'
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $labOutput = @(& $executable `
            --inject `
            --acknowledge-desktop-input `
            --vhf `
            --exit-after-release-exactly=1 `
            --anchor-and-observe-cursor 2>&1)
        $labExit = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    $labOutput | Write-Output
    if ($labExit -ne 0) {
        throw "Live Android VHF cursor acceptance failed: $labExit"
    }
    $labText = $labOutput -join [Environment]::NewLine
    if ($labText -notmatch '(?m)^accepted_exit_gesture_peak_contacts=1\s*$') {
        throw 'Cursor run did not accept an exact-one-contact gesture.'
    }
    if ($labText -notmatch '(?m)^cursor_moved=true\s*$') {
        throw 'Cursor run did not report a non-zero Windows cursor delta.'
    }
    Write-Output 'CAPY-PTP-003V live Android VHF cursor: PASS'
}
finally {
    Stop-Transcript
}
