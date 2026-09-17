[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string]$DisplayConfig,
    [Parameter(Mandatory = $true)] [string]$CaCertificate,
    [Parameter(Mandatory = $true)] [string]$ClientCertificate,
    [Parameter(Mandatory = $true)] [string]$ClientPrivateKey,
    [Parameter(Mandatory = $true)] [string]$Output,
    [string]$PropertiesGenerator = "target/debug/capyio-display-provisioning-properties.exe"
)

$ErrorActionPreference = 'Stop'
$generator = (Resolve-Path -LiteralPath $PropertiesGenerator -ErrorAction Stop).Path
$configuration = (Resolve-Path -LiteralPath $DisplayConfig -ErrorAction Stop).Path
$previousErrorAction = $ErrorActionPreference
try {
    $ErrorActionPreference = 'Continue'
    $lines = @(& $generator $configuration 2>&1)
    $generatorExitCode = $LASTEXITCODE
} finally {
    $ErrorActionPreference = $previousErrorAction
}
if ($generatorExitCode -ne 0) {
    throw 'The strict Windows service configuration parser rejected the display configuration.'
}
$properties = (($lines | ForEach-Object { $_.ToString() }) -join "`n") + "`n"
if ($properties -match "[`0`r]" -or $properties.Length -gt 8192) {
    throw 'The provisioning properties generator returned an invalid result.'
}

& (Join-Path $PSScriptRoot 'new_android_display_connection_bundle.ps1') `
    -Transport gamestream `
    -ConnectionPropertiesText $properties `
    -CaCertificate $CaCertificate `
    -ClientCertificate $ClientCertificate `
    -ClientPrivateKey $ClientPrivateKey `
    -Output $Output
