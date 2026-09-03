[CmdletBinding()]
param(
    [Parameter()]
    [string] $OutputDirectory = (Join-Path $PSScriptRoot 'bin')
)

$ErrorActionPreference = 'Stop'
$frameworkDirectory = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319'
$compiler = Join-Path $frameworkDirectory 'csc.exe'
$runtimeAssembly = Join-Path $frameworkDirectory 'System.Runtime.dll'
$windowsRuntimeAssembly = Join-Path $frameworkDirectory 'System.Runtime.WindowsRuntime.dll'
$unionMetadataRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\UnionMetadata'
$windowsMetadata = Get-ChildItem -LiteralPath $unionMetadataRoot -Directory |
    Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' } |
    Sort-Object { [version]$_.Name } -Descending |
    ForEach-Object { Join-Path $_.FullName 'Windows.winmd' } |
    Where-Object { Test-Path -LiteralPath $_ } |
    Select-Object -First 1

foreach ($required in @($compiler, $runtimeAssembly, $windowsRuntimeAssembly, $windowsMetadata)) {
    if (-not $required -or -not (Test-Path -LiteralPath $required)) {
        throw "Required Windows SDK/.NET Framework input is unavailable: $required"
    }
}

New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$output = Join-Path $OutputDirectory 'CapyIO.RawGameControllerProbe.exe'
$source = Join-Path $PSScriptRoot 'Program.cs'
& $compiler /nologo /target:exe /platform:x64 "/out:$output" "/reference:$runtimeAssembly" "/reference:$windowsRuntimeAssembly" "/reference:$windowsMetadata" $source
if ($LASTEXITCODE -ne 0) {
    throw "C# compiler failed with exit code $LASTEXITCODE"
}
$hidOutput = Join-Path $OutputDirectory 'CapyIO.HidReportProbe.exe'
$hidSource = Join-Path $PSScriptRoot 'HidReportProbe.cs'
& $compiler /nologo /target:exe /platform:x64 "/out:$hidOutput" $hidSource
if ($LASTEXITCODE -ne 0) {
    throw "C# HID probe compiler failed with exit code $LASTEXITCODE"
}
Write-Output $output
Write-Output $hidOutput
$xinputOutput = Join-Path $OutputDirectory 'CapyIO.XInputProbe.exe'
$xinputSource = Join-Path $PSScriptRoot 'XInputProbe.cs'
& $compiler /nologo /target:exe /platform:x64 "/out:$xinputOutput" $xinputSource
if ($LASTEXITCODE -ne 0) {
    throw "C# XInput probe compiler failed with exit code $LASTEXITCODE"
}
Write-Output $xinputOutput
