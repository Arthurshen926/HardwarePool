[CmdletBinding()]
param(
    [Parameter()]
    [string] $ViGEmManagedAssembly
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..\..')).Path
$defaultViGEmManagedAssembly = Join-Path $repositoryRoot 'target\physical-lab\vigem-client-1.21.256\unpacked\lib\netstandard2.0\Nefarius.ViGEm.Client.dll'
if ([string]::IsNullOrWhiteSpace($ViGEmManagedAssembly)) {
    $ViGEmManagedAssembly = $defaultViGEmManagedAssembly
}
$desktopOutput = Join-Path $repositoryRoot 'target\debug'
$buildSidecar = Join-Path $PSScriptRoot 'build.ps1'

Push-Location $repositoryRoot
try {
    & cargo build -p capyio-desktop
    if ($LASTEXITCODE -ne 0) {
        throw "Desktop build failed with exit code $LASTEXITCODE"
    }

    $sidecar = & $buildSidecar -ViGEmManagedAssembly $ViGEmManagedAssembly -OutputDirectory $desktopOutput
    if ($LASTEXITCODE -ne 0) {
        throw "ViGEm sidecar build failed with exit code $LASTEXITCODE"
    }

    $sidecarPath = [string] ($sidecar | Select-Object -Last 1)
    & $sidecarPath --self-test
    if ($LASTEXITCODE -ne 0) {
        throw "ViGEm sidecar self-test failed with exit code $LASTEXITCODE"
    }

    foreach ($required in @(
        (Join-Path $desktopOutput 'capyio-desktop.exe'),
        (Join-Path $desktopOutput 'CapyIO.ViGEmX360Sidecar.exe'),
        (Join-Path $desktopOutput 'Nefarius.ViGEm.Client.dll')
    )) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
            throw "Staged desktop gamepad runtime is incomplete: $required"
        }
    }

    Write-Output "CAPYIO_GAMEPAD_DESKTOP_DEBUG_STAGED=$desktopOutput"
}
finally {
    Pop-Location
}
