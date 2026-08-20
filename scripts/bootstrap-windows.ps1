[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

Write-Host "HardwarePool Windows bootstrap check (read-only)" -ForegroundColor Cyan

$commands = @(
    @{ Name = "git"; Args = @("--version"); Required = $true },
    @{ Name = "rustc"; Args = @("--version"); Required = $true },
    @{ Name = "cargo"; Args = @("--version"); Required = $true },
    @{ Name = "node"; Args = @("--version"); Required = $true },
    @{ Name = "corepack"; Args = @("--version"); Required = $true },
    @{ Name = "pnpm"; Args = @("--version"); Required = $false },
    @{ Name = "adb"; Args = @("version"); Required = $false },
    @{ Name = "msbuild"; Args = @("-version"); Required = $false },
    @{ Name = "windbg"; Args = @("-version"); Required = $false }
)

$missingRequired = @()
foreach ($command in $commands) {
    $resolved = Get-Command $command.Name -ErrorAction SilentlyContinue
    if ($null -eq $resolved) {
        $kind = if ($command.Required) { "REQUIRED" } else { "optional" }
        Write-Host ("{0,-12} MISSING ({1})" -f $command.Name, $kind) -ForegroundColor Yellow
        if ($command.Required) { $missingRequired += $command.Name }
        continue
    }

    try {
        $output = & $command.Name @($command.Args) 2>&1 | Select-Object -First 1
        Write-Host ("{0,-12} OK  {1}" -f $command.Name, $output) -ForegroundColor Green
    }
    catch {
        Write-Host ("{0,-12} FOUND but version check failed: {1}" -f $command.Name, $_) -ForegroundColor Yellow
    }
}

Write-Host "`nSystem inventory" -ForegroundColor Cyan
Get-ComputerInfo | Select-Object WindowsProductName, WindowsVersion, OsBuildNumber, OsArchitecture, CsSystemType | Format-List

if ($missingRequired.Count -gt 0) {
    throw "Missing required tools: $($missingRequired -join ', ')"
}

Write-Host "`nNo software was installed and no system setting was changed." -ForegroundColor Cyan
