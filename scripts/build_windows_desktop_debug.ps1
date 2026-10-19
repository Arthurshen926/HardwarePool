$ErrorActionPreference = 'Stop'
$repository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$desktop = Join-Path $repository 'apps\desktop'
$tauri = Join-Path $desktop 'node_modules\.bin\tauri.cmd'
$executable = Join-Path $repository 'target\debug\capyio-desktop.exe'

if (-not (Test-Path -LiteralPath $tauri -PathType Leaf)) {
    throw 'Tauri CLI is missing. Install the pinned desktop dependencies before building.'
}

Push-Location $desktop
try {
    & $tauri build --debug --no-bundle
    if ($LASTEXITCODE -ne 0) { throw "Tauri build failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}

if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "Desktop executable was not produced: $executable"
}

Get-Item -LiteralPath $executable | Select-Object FullName, Length, LastWriteTime
Get-FileHash -LiteralPath $executable -Algorithm SHA256 | Select-Object Algorithm, Hash
