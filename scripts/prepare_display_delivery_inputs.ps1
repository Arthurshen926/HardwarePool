[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$EngineDirectory,
    [Parameter(Mandatory)][string]$ResourceDirectory
)
# Explicit local runtime files only: no pairing files, deployment or downloads.
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent $PSScriptRoot
$output=Join-Path $repo ('target/display-delivery-inputs-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-ffff'))
$inputs=[ordered]@{}
foreach($name in @('libcapyio_display_engine.dll','libgcc_s_seh-1.dll','libstdc++-6.dll','libvpl-2.dll','libwinpthread-1.dll')) {
    $inputs[$name]=Join-Path $EngineDirectory $name
}
foreach($folder in @('assets','include','tools')) {
    foreach($file in Get-ChildItem -LiteralPath (Join-Path $ResourceDirectory $folder) -File -Recurse) {
        $relative=[IO.Path]::GetRelativePath($ResourceDirectory,$file.FullName).Replace('\','/')
        if($relative -match '^(tools/sunshine_wgc_capture\.exe|assets/apps\.json|assets/shaders/directx/[A-Za-z0-9_-]+\.hlsl|include/[A-Za-z0-9_-]+\.hlsl)$') { $inputs[$relative]=$file.FullName }
    }
}
foreach($path in $inputs.Values) { if(-not (Test-Path -LiteralPath $path -PathType Leaf)){throw "Missing runtime: $path"} }
$artifacts=@(foreach($entry in $inputs.GetEnumerator()) {
    $destination=Join-Path $output $entry.Key
    New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
    Copy-Item -LiteralPath $entry.Value -Destination $destination
    @{file=$entry.Key;sha256=(Get-FileHash -LiteralPath $destination).Hash}
})
$manifest=Join-Path $output 'package-inputs.json'
@{distribution='local-debug-only';licenseNotice='GPL display integration: local debug only, not cleared for public redistribution; see third_party/THIRD_PARTY.yml';artifacts=$artifacts} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifest -Encoding utf8
Write-Output $manifest
