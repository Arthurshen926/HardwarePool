[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\capyio-ptp-adb-lab.exe'
$expectedHash = '90DB46F014237935564FD6634F630EE77477B0AE00FAC5DB96EA53B35F3C6CA3'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003Z-android-vhf-persistent.txt'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The persistent Android-to-VHF lab session requires elevated PowerShell.'
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
    Write-Output 'acceptance=CAPY-PTP-003Z'
    Write-Output 'projection=android-adb-reverse-to-installed-vhf'
    Write-Output 'session_mode=continuous-until-android-close-or-600-second-idle'
    Write-Output 'android_apk_version=1.10'
    Write-Output 'android_apk_sha256=C80E7718D342C919959C2D45DB4F482DAD1444B35A25E43F1FDDFBEDF1BAA474'
    Write-Output 'capyio_must_remain_foreground=true'
    Write-Output 'separate_remote_viewer_cursor_rendering_out_of_scope=true'
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
            --manual-session 2>&1)
        $labExit = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    $labOutput | Write-Output
    if ($labExit -ne 0) {
        throw "Persistent Android-to-VHF lab session failed: $labExit"
    }
    $labText = $labOutput -join [Environment]::NewLine
    if ($labText -notmatch '(?m)^max_contacts_observed=([1-5])\s*$') {
        throw 'Persistent session did not observe physical touch input.'
    }
    Write-Output 'CAPY-PTP-003Z persistent Android-to-VHF session: PASS'
}
finally {
    Stop-Transcript
}
