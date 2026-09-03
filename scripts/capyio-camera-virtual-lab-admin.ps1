#Requires -RunAsAdministrator

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('Deploy', 'Remove', 'RemoveWithFrameServerRestart')]
    [string]$Action
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$expectedSha256 = '4C236858C5223B4A1303E825496EBE6799C52E9EAE366DC6DE41C8E9A88F70F0'
$sourceDll = Join-Path $PSScriptRoot '..\target\release\capyio_windows_camera_mf.dll'
$capyioRoot = 'C:\ProgramData\CapyIO'
$labRoot = Join-Path $capyioRoot 'Lab'
$deployDirectory = 'C:\ProgramData\CapyIO\Lab\Camera'
$deployDll = Join-Path $deployDirectory 'capyio_windows_camera_mf.dll'
$clsid = '{35754be3-54b6-4133-a1c7-1716395c6f1c}'
$clsidKey = "Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Classes\CLSID\$clsid"
$serverKey = Join-Path $clsidKey 'InprocServer32'

function Remove-EmptyDirectory {
    param([Parameter(Mandatory = $true)][string]$Path)

    if ((Test-Path -LiteralPath $Path) -and
        ((Get-ChildItem -LiteralPath $Path -Force | Measure-Object).Count -eq 0)) {
        Remove-Item -LiteralPath $Path -Force
    }
}

function Remove-LabRegistration {
    if (Test-Path -LiteralPath $clsidKey) {
        Remove-Item -LiteralPath $clsidKey -Recurse -Force
    }
    if (Test-Path -LiteralPath $deployDll) {
        Remove-Item -LiteralPath $deployDll -Force
    }
    Remove-EmptyDirectory -Path $deployDirectory
    Remove-EmptyDirectory -Path $labRoot
    Remove-EmptyDirectory -Path $capyioRoot
}

if ($Action -eq 'Remove') {
    Remove-LabRegistration
    Write-Output 'admin_cleanup=pass'
    exit 0
}

if ($Action -eq 'RemoveWithFrameServerRestart') {
    $frameServer = Get-Service -Name 'FrameServer'
    $restartFrameServer = $frameServer.Status -eq [System.ServiceProcess.ServiceControllerStatus]::Running
    try {
        if ($restartFrameServer) {
            Stop-Service -Name 'FrameServer'
            $frameServer.WaitForStatus(
                [System.ServiceProcess.ServiceControllerStatus]::Stopped,
                [TimeSpan]::FromSeconds(15))
        }
        Remove-LabRegistration
    }
    finally {
        if ($restartFrameServer) {
            Start-Service -Name 'FrameServer'
            $frameServer.WaitForStatus(
                [System.ServiceProcess.ServiceControllerStatus]::Running,
                [TimeSpan]::FromSeconds(15))
        }
    }
    Write-Output 'admin_cleanup=pass'
    Write-Output 'frameserver_restart=pass'
    exit 0
}

if (-not (Test-Path -LiteralPath $sourceDll -PathType Leaf)) {
    throw "Release DLL does not exist: $sourceDll"
}
$actualSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourceDll).Hash
if ($actualSha256 -ne $expectedSha256) {
    throw "Release DLL SHA-256 mismatch: expected $expectedSha256, got $actualSha256"
}
if (Test-Path -LiteralPath $deployDll) {
    throw "Refusing to overwrite existing lab DLL: $deployDll"
}
if (Test-Path -LiteralPath $clsidKey) {
    throw "Refusing to overwrite existing COM registration: $clsidKey"
}
if (Test-Path -LiteralPath $capyioRoot) {
    throw "Refusing to reuse existing CapyIO ProgramData root: $capyioRoot"
}

$copiedDll = $false
$createdRegistryKey = $false
try {
    New-Item -ItemType Directory -Path $deployDirectory | Out-Null
    Copy-Item -LiteralPath $sourceDll -Destination $deployDll
    $copiedDll = $true

    $icacls = Join-Path ([Environment]::SystemDirectory) 'icacls.exe'
    foreach ($directory in @($capyioRoot, $labRoot, $deployDirectory)) {
        & $icacls $directory /grant '*S-1-5-19:(OI)(CI)(RX)' | Out-Null
        if ($LASTEXITCODE -ne 0) {
            throw "icacls failed for $directory with exit code $LASTEXITCODE"
        }
    }
    & $icacls $deployDll /grant '*S-1-5-19:(RX)' | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "icacls failed for lab DLL with exit code $LASTEXITCODE"
    }

    New-Item -Path $clsidKey | Out-Null
    $createdRegistryKey = $true
    Set-Item -LiteralPath $clsidKey -Value 'CapyIO Virtual Camera Media Source'
    New-Item -Path $serverKey | Out-Null
    Set-Item -LiteralPath $serverKey -Value $deployDll
    New-ItemProperty -LiteralPath $serverKey -Name 'ThreadingModel' -PropertyType String -Value 'Both' | Out-Null

    $deployedSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $deployDll).Hash
    if ($deployedSha256 -ne $expectedSha256) {
        throw "Deployed DLL SHA-256 mismatch: expected $expectedSha256, got $deployedSha256"
    }

    Write-Output 'admin_deploy=pass'
    Write-Output "sha256=$deployedSha256"
    Write-Output "clsid=$clsid"
}
catch {
    if ($createdRegistryKey -and (Test-Path -LiteralPath $clsidKey)) {
        Remove-Item -LiteralPath $clsidKey -Recurse -Force
    }
    if ($copiedDll -and (Test-Path -LiteralPath $deployDll)) {
        Remove-Item -LiteralPath $deployDll -Force
    }
    Remove-EmptyDirectory -Path $deployDirectory
    Remove-EmptyDirectory -Path $labRoot
    Remove-EmptyDirectory -Path $capyioRoot
    throw
}
