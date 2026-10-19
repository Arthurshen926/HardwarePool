[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PackageRoot,
    [string]$MakeNsis = (Join-Path $env:LOCALAPPDATA 'tauri\NSIS\makensis.exe')
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not (Test-Path -LiteralPath $MakeNsis -PathType Leaf)) { throw 'NSIS is not available. Supply the existing Tauri NSIS makensis.exe; no automatic download is performed.' }
& (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $PackageRoot
$bundle = (Get-Item -LiteralPath $PackageRoot).FullName
if ($bundle -match '["$\r\n]') { throw 'Package path contains unsupported NSIS metacharacters.' }
$buildId = 'local-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-ffff')
$output = Join-Path (Split-Path -Parent $bundle) "CapyIO-$buildId-setup.exe"
& $MakeNsis '/V2' "/DBUNDLE_ROOT=$bundle" "/DOUTPUT_FILE=$output" "/DBUILD_ID=$buildId" (Join-Path $PSScriptRoot 'local_debug_installer.nsi')
if ($LASTEXITCODE -ne 0) { throw 'NSIS compilation failed.' }
Get-FileHash -LiteralPath $output -Algorithm SHA256
Write-Output "Unsigned local application installer: $output"
Write-Output 'Not executed. Does not install drivers/services or satisfy public distribution/signing requirements.'
