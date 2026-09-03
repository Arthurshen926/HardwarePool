[CmdletBinding()]
param(
    [Parameter()]
    [string] $ViGEmManagedAssembly = (Join-Path $PSScriptRoot '..\..\..\..\..\target\physical-lab\vigem-client-1.21.256\unpacked\lib\netstandard2.0\Nefarius.ViGEm.Client.dll'),

    [Parameter()]
    [string] $OutputDirectory = (Join-Path $PSScriptRoot 'bin')
)

$ErrorActionPreference = 'Stop'
$frameworkDirectory = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319'
$compiler = Join-Path $frameworkDirectory 'csc.exe'
$systemRuntime = Join-Path $frameworkDirectory 'System.Runtime.dll'
$netStandard = Join-Path $frameworkDirectory 'netstandard.dll'

foreach ($required in @($compiler, $systemRuntime, $netStandard, $ViGEmManagedAssembly)) {
    if (-not $required -or -not (Test-Path -LiteralPath $required)) {
        throw "Required ViGEm sidecar build input is unavailable: $required"
    }
}

New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$output = Join-Path $OutputDirectory 'CapyIO.ViGEmX360Sidecar.exe'
$source = Join-Path $PSScriptRoot 'Program.cs'
& $compiler /nologo /target:exe /platform:x64 "/out:$output" "/reference:$systemRuntime" "/reference:$netStandard" "/reference:$ViGEmManagedAssembly" $source
if ($LASTEXITCODE -ne 0) {
    throw "C# compiler failed with exit code $LASTEXITCODE"
}
Copy-Item -LiteralPath $ViGEmManagedAssembly -Destination (Join-Path $OutputDirectory 'Nefarius.ViGEm.Client.dll') -Force
Write-Output $output
