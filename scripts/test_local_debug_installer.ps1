$ErrorActionPreference = 'Stop'
# Real file-only installer, fake payloads, isolated workspace target. No app/driver/APK launch.
$repository = Split-Path -Parent $PSScriptRoot
$checks = @(& (Join-Path $PSScriptRoot 'test_local_debug_bundle.ps1'))
$checks | Write-Output
$line = $checks | Where-Object { $_ -like 'PASS:*Fake fixtures retained at *' } | Select-Object -Last 1
if (-not $line) { throw 'Bundle fixture check did not complete.' }
$fixture = $line.Substring($line.IndexOf('Fake fixtures retained at ') + 'Fake fixtures retained at '.Length)
$package = Join-Path $fixture 'package'
# Restore the deliberately tampered fixture to its original, manifest-checked bytes.
Set-Content -LiteralPath (Join-Path $package 'libcapyio_display_engine.dll') -Value 'fake-libcapyio_display_engine.dll'
& (Join-Path $PSScriptRoot 'build_local_debug_installer.ps1') -PackageRoot $package
$setup = Get-ChildItem -LiteralPath $fixture -Filter '*-setup.exe' | Select-Object -First 1
$parent = Join-Path $repository ('target/installer-check-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $parent | Out-Null
$target = [IO.Path]::GetFullPath((Join-Path $parent 'application'))
$workspaceTarget = [IO.Path]::GetFullPath((Join-Path $repository 'target')) + [IO.Path]::DirectorySeparatorChar
if (-not $target.StartsWith($workspaceTarget, [StringComparison]::OrdinalIgnoreCase)) { throw 'Test target escaped workspace.' }
$installer = Start-Process -FilePath $setup.FullName -ArgumentList "/S /NOSHORTCUT /D=$target" -WindowStyle Hidden -PassThru
if (-not $installer.WaitForExit(30000) -or $installer.ExitCode -ne 0) { throw 'File installer failed.' }
& (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $target
# A second install must refuse the existing version directory.
$again = Start-Process -FilePath $setup.FullName -ArgumentList "/S /NOSHORTCUT /D=$target" -WindowStyle Hidden -PassThru
if (-not $again.WaitForExit(30000) -or $again.ExitCode -ne 2) { throw 'Installer did not reject an existing target.' }
& (Join-Path $PSScriptRoot 'remove_local_debug_bundle.ps1') -TargetDirectory $target -Apply
$archive = @(Get-ChildItem -LiteralPath $parent -Directory -Filter 'application.removed-*')
if ((Test-Path -LiteralPath $target) -or $archive.Count -ne 1) { throw 'Recoverable removal failed.' }
& (Join-Path $PSScriptRoot 'install_local_debug_bundle.ps1') -PackageRoot $archive[0].FullName -TargetDirectory $target -Apply
& (Join-Path $PSScriptRoot 'verify_local_debug_bundle.ps1') -PackageRoot $target
Write-Output "PASS: NSIS file installation, existing-target rejection, recoverable removal and restore. Fake payloads retained at $parent"
