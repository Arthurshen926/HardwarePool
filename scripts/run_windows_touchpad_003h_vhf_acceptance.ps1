[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\deps\touchpad_runtime_worker-1ef346b5542e36f7.exe'
$expectedHash = 'F7F16EA7F903D1721D2D441B90B5816C19981A94DFA1A242C2C76985F5DD0A7C'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003H-vhf-open-close.txt'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'VHF acceptance requires an elevated administrator PowerShell.'
}
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "Exact acceptance executable is absent: $executable"
}
$actualHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash
if ($actualHash -ne $expectedHash) {
    throw "Acceptance executable hash mismatch: $actualHash"
}

New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Start-Transcript -LiteralPath $evidencePath -Force
try {
    & $executable `
        authorized_vhf_factory_opens_and_closes_real_driver_interface_without_frames `
        --ignored --exact --nocapture
    $testExit = $LASTEXITCODE
    if ($testExit -ne 0) {
        throw "Exact VHF Hello/Close acceptance failed: $testExit"
    }
    Write-Output 'CAPY-PTP-003H exact VHF Hello/Close acceptance: PASS; frames_submitted=0'
}
finally {
    Stop-Transcript
}
