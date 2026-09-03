[CmdletBinding()]
param()

$ErrorActionPreference = 'Continue'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003F-recovery.txt'
New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null

Start-Transcript -LiteralPath $evidencePath -Force
try {
    Write-Output 'CAPY-PTP-003F elevated read-only recovery inventory'
    Write-Output "captured_at=$((Get-Date).ToString('o'))"
    whoami
    hostname
    Write-Output '--- WinRE ---'
    reagentc.exe /info
    Write-Output '--- BitLocker C: ---'
    manage-bde.exe -status C:
    Write-Output '--- Secure Boot ---'
    Confirm-SecureBootUEFI
    Write-Output '--- TPM ---'
    Get-Tpm | Format-List
    Write-Output '--- OS ---'
    Get-CimInstance Win32_OperatingSystem |
        Select-Object Caption, Version, BuildNumber, OSArchitecture, LastBootUpTime |
        Format-List
    Write-Output '--- Restore points ---'
    Get-ComputerRestorePoint |
        Select-Object SequenceNumber, Description, CreationTime, RestorePointType |
        Format-Table -AutoSize
}
finally {
    Stop-Transcript
}
