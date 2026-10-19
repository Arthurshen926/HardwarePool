[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-fA-F-]{36}$')]
    [string]$NodeId,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$')]
    [string]$HostAddress,

    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 65535)]
    [int]$ControlPort
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\capyio-desktop.exe'
$expectedHash = 'A96EBDE203F0E85E9AF59F321A12E1F015ABC44E38160AE40FE193A822CABDEE'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-INTEGRATED-paired-touchpad-data.txt'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Paired touchpad data acceptance requires elevated PowerShell.'
}
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "CapyIO Desktop is absent: $executable"
}
$actualHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash
if ($actualHash -ne $expectedHash) {
    throw "CapyIO Desktop hash mismatch: $actualHash"
}

New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Start-Transcript -LiteralPath $evidencePath -Force
try {
    Write-Output "desktop_sha256=$actualHash"
    Write-Output "target_node=$NodeId"
    Write-Output "target_endpoint=$HostAddress`:$ControlPort"
    Write-Output 'desktop_input_acknowledged=true'
    & $executable `
        --verify-paired-touchpad-data `
        $NodeId `
        $HostAddress `
        $ControlPort `
        --acknowledge-desktop-input
    if ($LASTEXITCODE -ne 0) {
        throw "Paired touchpad data gate failed: $LASTEXITCODE"
    }
}
finally {
    Stop-Transcript
}
