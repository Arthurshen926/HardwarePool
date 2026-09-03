[CmdletBinding()]
param()

$ErrorActionPreference = 'Continue'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003F-signing-state.txt'
New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null

Start-Transcript -LiteralPath $evidencePath -Force
try {
    Write-Output 'CAPY-PTP-003F elevated read-only signing-state inventory'
    Write-Output "captured_at=$((Get-Date).ToString('o'))"
    Write-Output '--- Current boot entry ---'
    bcdedit.exe /enum '{current}'
    Write-Output '--- Code Integrity policy ---'
    Get-CimInstance -Namespace root\Microsoft\Windows\DeviceGuard -ClassName Win32_DeviceGuard |
        Format-List
    Write-Output '--- Current test certificate trust ---'
    $subject = 'CN=CapyIO Local Lab Driver Test 003F'
    foreach ($store in @('Cert:\LocalMachine\Root', 'Cert:\LocalMachine\TrustedPublisher')) {
        $matches = @(Get-ChildItem -LiteralPath $store | Where-Object Subject -eq $subject)
        Write-Output "$store matches=$($matches.Count)"
    }
}
finally {
    Stop-Transcript
}
