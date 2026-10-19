[CmdletBinding()]
param([Parameter(Mandatory)][string]$TargetDirectory, [switch]$Apply)
# Recoverable application removal only. Never deletes user data or system components.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$target = (Get-Item -LiteralPath $TargetDirectory).FullName.TrimEnd('\','/')
& (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $target -AllowRollbackBackups
$ancestor = Get-Item -LiteralPath $target
while ($null -ne $ancestor) {
    if ($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Removal path cannot traverse a link.' }
    $ancestor = $ancestor.Parent
}
# Failure to inspect services is not permission to move their binaries.
$services = @(Get-CimInstance Win32_Service -ErrorAction Stop)
foreach ($service in $services) {
    if ($service.PathName -and $service.PathName.IndexOf($target + '\', [StringComparison]::OrdinalIgnoreCase) -ge 0) {
        throw "A registered service uses this directory: $($service.Name). Use its separate approved removal process first."
    }
}
foreach ($process in Get-Process) {
    if ($process.Path -and $process.Path.StartsWith($target + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Use Node > Exit node (closing the window keeps the node running): $($process.ProcessName)"
    }
}
$archive = $target + '.removed-' + [Guid]::NewGuid().ToString('N')
$shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) 'CapyIO Local Debug.lnk'
$ownedShortcut = $false
if (Test-Path -LiteralPath $shortcutPath -PathType Leaf) {
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($shortcutPath)
    $ownedShortcut = $shortcut.TargetPath -eq (Join-Path $target 'capyio-desktop.exe')
}
Write-Output "Plan: archive application directory to $archive; archive matching shortcut only. Keep pairing, settings, drivers, services and Android app."
if (-not $Apply) { Write-Output 'No changes. Repeat with -Apply to remove this application version recoverably.'; return }
# Both paths are explicit verified siblings, never a recursive delete.
Move-Item -LiteralPath $target -Destination $archive
if ($ownedShortcut) {
    try { Move-Item -LiteralPath $shortcutPath -Destination ($archive + '.lnk') }
    catch { Write-Warning 'Application archived, but the old shortcut could not be moved. Remove that stale shortcut manually.' }
}
Write-Output "Application version removed from its original path; recoverable files retained: $archive"
Write-Output 'To restore, use install_local_debug_bundle.ps1 with this archive as PackageRoot and the original TargetDirectory. No disk space was reclaimed.'
