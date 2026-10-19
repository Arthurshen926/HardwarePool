[CmdletBinding()]
param(
    [ValidateSet('Upgrade','Rollback')][string]$Mode='Upgrade',
    [string]$PackageRoot,
    [Parameter(Mandatory)][string]$TargetDirectory,
    [ValidatePattern('^[0-9]{8}-[0-9]{6}-[0-9]{4}$')][string]$BackupName,
    [switch]$Apply
)
# Explicit local files only. Default prints a plan; never touches installed services/APKs/drivers/settings.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$target=[IO.Path]::GetFullPath($TargetDirectory).TrimEnd('\','/')
$repo=Split-Path -Parent $PSScriptRoot
if ($target -eq [IO.Path]::GetPathRoot($target).TrimEnd('\','/') -or $target -eq $env:USERPROFILE -or $target -eq $repo) { throw 'Choose a dedicated application directory, not a filesystem/home/workspace root.' }
if (-not (Test-Path -LiteralPath $target -PathType Container)) { throw 'Create/select the dedicated target directory first.' }
if ((Get-Item -LiteralPath $target).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Target directory cannot be a link.' }
$broker=Get-CimInstance Win32_Service -Filter "Name='CapyIOBroker'" -ErrorAction SilentlyContinue
if ($broker -and $broker.PathName -match [regex]::Escape((Join-Path $target 'capyio-windows-service.exe'))) { throw 'Target contains the registered Broker. Use the separate approved service deployment workflow; this updater does not change it.' }
foreach ($process in Get-Process) {
    $path=$process.Path
    if ($path -and $path.StartsWith($target + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw "Use Node > Exit node (closing the window keeps the node running), then stop target helpers: $($process.ProcessName)" }
}
$backupRoot=Join-Path $target '.capyio-backups'
$runtimePattern = '^([A-Za-z0-9_+.-]+\.(exe|dll)|tools/sunshine_wgc_capture\.exe|assets/apps\.json|assets/shaders/directx/[A-Za-z0-9_-]+\.hlsl|include/[A-Za-z0-9_-]+\.hlsl)$'
function Assert-OrdinaryParents([string]$rootPath, [string]$relative) {
    $current = $rootPath
    foreach ($part in $relative.Split('/')) {
        $current = Join-Path $current $part
        if ((Test-Path -LiteralPath $current) -and ((Get-Item -LiteralPath $current).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Runtime paths cannot traverse links.' }
    }
}
if ((Test-Path -LiteralPath $backupRoot) -and ((Get-Item -LiteralPath $backupRoot).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Backup directory cannot be a link.' }
if ($Mode -eq 'Upgrade') {
    if (-not $PackageRoot) { throw 'Upgrade requires PackageRoot.' }
    & (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $PackageRoot
    $manifest=Get-Content -LiteralPath (Join-Path $PackageRoot 'manifest.json') -Raw | ConvertFrom-Json
    if (Test-Path -LiteralPath (Join-Path $target 'manifest.json')) {
        $current = Get-Content -LiteralPath (Join-Path $target 'manifest.json') -Raw | ConvertFrom-Json
        $from = if ($current.PSObject.Properties['profile']) { $current.profile } else { 'integrated' }
        $to = if ($manifest.PSObject.Properties['profile']) { $manifest.profile } else { 'integrated' }
        if ($from -ne $to) { throw 'Profile migration requires a new application directory; runtime-only upgrade cannot remove optional components.' }
    }
    $entries=@($manifest.artifacts | Where-Object { $_.file -match $runtimePattern })
    $installedManifest = Join-Path $target 'manifest.json'
    $installed = $null
    if (Test-Path -LiteralPath $installedManifest) {
        & (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $target -AllowRollbackBackups
        $installed = Get-Content -LiteralPath $installedManifest -Raw | ConvertFrom-Json
    }
    foreach ($entry in $entries) { Assert-OrdinaryParents $target $entry.file }
    Write-Output "Plan: replace $($entries.Count) runtime files in $target; preserve all pairing/settings; do not install APK or register services."
    if (-not $Apply) { Write-Output 'No changes. Re-run with -Apply to execute this plan.'; return }
    $BackupName=[DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-ffff')
    $backup=Join-Path $backupRoot $BackupName
    New-Item -ItemType Directory -Path $backup | Out-Null
    $records=@(foreach ($entry in $entries) {
        $old=Join-Path $target $entry.file
        New-Item -ItemType Directory -Path (Split-Path -Parent (Join-Path $backup $entry.file)) -Force | Out-Null
        if ((Test-Path -LiteralPath $old) -and -not (Test-Path -LiteralPath $old -PathType Leaf)) { throw 'A runtime filename is occupied by a directory.' }
        $present=Test-Path -LiteralPath $old -PathType Leaf
        if ($present) {
            if ((Get-Item -LiteralPath $old).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Target file cannot be a link.' }
            Copy-Item -LiteralPath $old -Destination (Join-Path $backup $entry.file)
        }
        @{file=$entry.file; existed=$present; oldHash=$(if ($present) { (Get-FileHash -LiteralPath (Join-Path $backup $entry.file)).Hash } else { $null }); newHash=$entry.sha256}
    })
    if ($null -ne $installed) {
        Copy-Item -LiteralPath $installedManifest -Destination (Join-Path $backup 'manifest.json')
        $runtimeNames = @($entries | ForEach-Object { $_.file })
        $installed.artifacts = @($installed.artifacts | Where-Object { $_.file -notin $runtimeNames }) + $entries
        $installed | Add-Member -Force NoteProperty runtimeUpgrade @{packageManifestSha256=(Get-FileHash -LiteralPath (Join-Path $PackageRoot 'manifest.json')).Hash; appliedAtUtc=[DateTime]::UtcNow.ToString('o'); scope='runtime-files-only'}
        $stagedManifest = Join-Path $backup 'upgraded-manifest.json'
        $installed | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $stagedManifest -Encoding UTF8
        $records += @{file='manifest.json';existed=$true;oldHash=(Get-FileHash -LiteralPath $installedManifest).Hash;newHash=(Get-FileHash -LiteralPath $stagedManifest).Hash}
    }
    @{schemaVersion=1;target=$target;files=$records} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $backup 'rollback.json') -Encoding UTF8
    try {
        foreach ($record in $records) {
            $path=Join-Path $target $record.file
            New-Item -ItemType Directory -Path (Split-Path -Parent $path) -Force | Out-Null
            $replacement = if ($record.file -eq 'manifest.json') { $stagedManifest } else { Join-Path $PackageRoot $record.file }
            Copy-Item -LiteralPath $replacement -Destination $path -Force
            if ((Get-FileHash -LiteralPath $path).Hash -ne $record.newHash) { throw "Copy verification failed: $($record.file)" }
        }
        if ($null -ne $installed) {
            & (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $target -AllowRollbackBackups
        }
    } catch {
        $failure=$_
        & $PSCommandPath -Mode Rollback -TargetDirectory $target -BackupName $BackupName -Apply
        throw "Upgrade failed; rollback attempted. Original error: $failure"
    }
    Write-Output "Upgraded files only. Rollback: -Mode Rollback -TargetDirectory '$target' -BackupName '$BackupName' -Apply"
} else {
    if (-not $BackupName) { throw 'Rollback requires the exact BackupName printed by upgrade.' }
    $backup=Join-Path $backupRoot $BackupName
    if ((Get-Item -LiteralPath $backup).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Backup cannot be a link.' }
    $receipt=Get-Content -LiteralPath (Join-Path $backup 'rollback.json') -Raw | ConvertFrom-Json
    if ($receipt.schemaVersion -ne 1 -or $receipt.target -ne $target) { throw 'Rollback target mismatch.' }
    foreach ($record in $receipt.files) {
        if ($record.file -ne 'manifest.json' -and $record.file -notmatch $runtimePattern) { throw 'Invalid rollback filename.' }
        Assert-OrdinaryParents $target $record.file
        Assert-OrdinaryParents $backup $record.file
        if ($record.existed) {
            $saved=Get-Item -LiteralPath (Join-Path $backup $record.file)
            if ($saved.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Backup files cannot be links.' }
            if ((Get-FileHash -LiteralPath $saved.FullName).Hash -ne $record.oldHash) { throw 'Backup hash mismatch; no rollback applied.' }
        }
    }
    Write-Output "Plan: restore recorded files from $backup; keep user settings and backup."
    if (-not $Apply) { Write-Output 'No changes. Re-run with -Apply to execute this plan.'; return }
    foreach ($record in $receipt.files) {
        $path=Join-Path $target $record.file
        if ((Test-Path -LiteralPath $path) -and ((Get-Item -LiteralPath $path).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Target file cannot be a link.' }
        if ($record.existed) {
            Copy-Item -LiteralPath (Join-Path $backup $record.file) -Destination $path -Force
            if ((Get-FileHash -LiteralPath $path).Hash -ne $record.oldHash) { throw "Rollback verification failed: $($record.file). Backup retained." }
        }
        elseif (Test-Path -LiteralPath $path) { Move-Item -LiteralPath $path -Destination (Join-Path $backup ($record.file + '.new-' + [Guid]::NewGuid().ToString('N'))) }
    }
    Write-Output 'Rollback complete. Added files were moved into the backup directory and remain recoverable.'
}
Write-Output "ONLY application entry: $(Join-Path $target 'capyio-desktop.exe')"
