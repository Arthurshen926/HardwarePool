[CmdletBinding()]
param(
    [string]$PackageRoot = $PSScriptRoot,
    [Parameter(Mandatory)][string]$TargetDirectory,
    [switch]$Apply
)
# Local application files only; no elevation, launch, driver or service changes.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$source = (Get-Item -LiteralPath $PackageRoot).FullName.TrimEnd('\','/')
$target = [IO.Path]::GetFullPath($TargetDirectory).TrimEnd('\','/')
if (Test-Path -LiteralPath $target) { throw 'Target already exists. Choose a new version directory or use the documented upgrade/rollback tool.' }
if ($target.StartsWith($source + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Install outside the source package.' }
$parent = Split-Path -Parent $target
if (-not (Test-Path -LiteralPath $parent -PathType Container)) { throw 'Choose an existing parent folder for the new application directory.' }
$ancestor = Get-Item -LiteralPath $parent
while ($null -ne $ancestor) {
    if ($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Installation path cannot traverse a filesystem link.' }
    $ancestor = $ancestor.Parent
}
& (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $source -AllowRollbackBackups
$manifest = Get-Content -LiteralPath (Join-Path $source 'manifest.json') -Raw | ConvertFrom-Json
Write-Output "Plan: copy $(@($manifest.artifacts).Count) verified application files plus manifest to $target."
Write-Output 'Pairing, settings, drivers, registered services and Android installation are unchanged. Existing system components are still required.'
if (-not $Apply) { Write-Output 'No changes. Review the destination, then repeat with -Apply.'; return }
New-Item -ItemType Directory -Path $target | Out-Null
try {
    foreach ($entry in $manifest.artifacts) {
        $destination = Join-Path $target $entry.file
        New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $source $entry.file) -Destination $destination
    }
    Copy-Item -LiteralPath (Join-Path $source 'manifest.json') -Destination (Join-Path $target 'manifest.json')
    & (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $target
} catch {
    throw "Installation incomplete; do not launch from '$target'. Partial files retained for inspection, existing installation untouched. $($_.Exception.Message)"
}
Write-Output "Application files installed. Open with your original Windows account: $(Join-Path $target 'capyio-desktop.exe')"
