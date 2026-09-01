[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$expectedArtifacts = [ordered]@{
    'capyio-avc-lab-receiver.exe' = '85C2164E3530F2790AB092D07EE5C7C82C5A106CA3F99A6F71B580703E4F149B'
    'capyio-camera-virtual-lab.exe' = '6FE25371377680761C3A0F65F0BD5048AF3B796C63D20F1136C4CFABDB846CD8'
    'capyio_windows_camera_mf.dll' = '4C236858C5223B4A1303E825496EBE6799C52E9EAE366DC6DE41C8E9A88F70F0'
}
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$releaseRoot = Join-Path $repoRoot 'target\release'
$adminScript = Join-Path $PSScriptRoot 'capyio-camera-virtual-lab-admin.ps1'
$capyioRoot = 'C:\ProgramData\CapyIO'
$clsidKey = 'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Classes\CLSID\{35754be3-54b6-4133-a1c7-1716395c6f1c}'
$labProcessNames = @('capyio-avc-lab-receiver', 'capyio-camera-virtual-lab')

foreach ($entry in $expectedArtifacts.GetEnumerator()) {
    $path = Join-Path $releaseRoot $entry.Key
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required release artifact is missing: $path"
    }
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash
    if ($actual -ne $entry.Value) {
        throw "Release artifact SHA-256 mismatch for $($entry.Key): expected $($entry.Value), got $actual"
    }
    Write-Output "artifact=$($entry.Key) sha256=$actual"
}

if (-not (Test-Path -LiteralPath $adminScript -PathType Leaf)) {
    throw "Camera lab deployment script is missing: $adminScript"
}
$expectedDllHash = $expectedArtifacts['capyio_windows_camera_mf.dll']
$adminText = Get-Content -LiteralPath $adminScript -Raw
if ($adminText -notmatch [regex]::Escape("`$expectedSha256 = '$expectedDllHash'")) {
    throw 'Camera lab deployment script does not pin the verified COM DLL hash'
}
Write-Output 'deployment_hash_lock=pass'

if (Test-Path -LiteralPath $capyioRoot) {
    throw "Refusing dirty ProgramData state: $capyioRoot exists"
}
if (Test-Path -LiteralPath $clsidKey) {
    throw "Refusing dirty COM state: $clsidKey exists"
}
Write-Output 'deployment_state=clean'

$listeners = @(Get-NetTCPConnection -State Listen -LocalPort 38173 -ErrorAction SilentlyContinue)
if ($listeners.Count -ne 0) {
    throw 'Refusing occupied camera receiver port: TCP 38173 has a listener'
}
Write-Output 'receiver_port=clean'

$running = @(Get-Process -Name $labProcessNames -ErrorAction SilentlyContinue)
if ($running.Count -ne 0) {
    $names = ($running | Select-Object -ExpandProperty ProcessName -Unique) -join ','
    throw "Refusing active camera lab processes: $names"
}
Write-Output 'lab_processes=clean'
Write-Output 'camera_live_lab_preflight=pass'
