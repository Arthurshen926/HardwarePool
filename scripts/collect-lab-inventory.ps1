[CmdletBinding()]
param(
    [string]$OutputDirectory = ".\test-results\inventory"
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

Get-ComputerInfo | Out-File (Join-Path $OutputDirectory "windows-computer-info.txt")
systeminfo | Out-File (Join-Path $OutputDirectory "windows-systeminfo.txt")
Get-CimInstance Win32_ComputerSystem | Format-List * | Out-File (Join-Path $OutputDirectory "windows-computer-system.txt")
Get-NetAdapter | Format-Table -AutoSize | Out-File (Join-Path $OutputDirectory "windows-network-adapters.txt")

if (Get-Command adb -ErrorAction SilentlyContinue) {
    adb devices | Out-File (Join-Path $OutputDirectory "adb-devices.txt")
    adb shell getprop ro.product.model | Out-File (Join-Path $OutputDirectory "android-model.txt")
    adb shell getprop ro.build.version.release | Out-File (Join-Path $OutputDirectory "android-release.txt")
    adb shell getprop ro.build.version.sdk | Out-File (Join-Path $OutputDirectory "android-api.txt")
    adb shell getprop ro.build.fingerprint | Out-File (Join-Path $OutputDirectory "android-build-fingerprint.txt")
}
else {
    "ADB not found" | Out-File (Join-Path $OutputDirectory "android-not-collected.txt")
}

Write-Host "Inventory written to $OutputDirectory"
Write-Warning "Review and sanitize identifiers before sharing these files publicly."
