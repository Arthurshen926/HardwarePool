[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\capyio-ptp-adb-lab.exe'
$expectedHash = 'E0123E541C245F5A90B82CFA761729C1EF56C9352E3C4D315AFB34312A1D59A3'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'integrated-android-vhf.txt'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The integrated VHF receiver requires elevated PowerShell.'
}
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "Integrated touchpad receiver is absent: $executable"
}
$actualHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash
if ($actualHash -ne $expectedHash) {
    throw "Integrated touchpad receiver hash mismatch: $actualHash"
}

New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Start-Transcript -LiteralPath $evidencePath -Force
try {
    Write-Output 'projection=unified-android-adb-reverse-to-installed-vhf'
    Write-Output "receiver_sha256=$actualHash"
    Write-Output 'driver_installation_performed=false'
    & $executable --inject --acknowledge-desktop-input --vhf --manual-session
    if ($LASTEXITCODE -ne 0) {
        throw "Integrated VHF receiver failed: $LASTEXITCODE"
    }
}
finally {
    Stop-Transcript
}
