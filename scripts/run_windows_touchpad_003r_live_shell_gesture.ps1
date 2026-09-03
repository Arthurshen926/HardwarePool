[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('three', 'four')]
    [string]$Gesture
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\capyio-ptp-adb-lab.exe'
$expectedHash = '65C96C37EB14513E08C55116EA9B52DFB18AE21B7E137218E90BD2652B6C820B'
$targetScript = Join-Path $PSScriptRoot 'windows-touchpad-gesture-target.ps1'
$slice = if ($Gesture -eq 'three') { '003R' } else { '003S' }
$requiredContacts = if ($Gesture -eq 'three') { 3 } else { 4 }
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot "CAPY-PTP-$slice-android-vhf-$Gesture-shell.txt"
$targetOutput = Join-Path $evidenceRoot "CAPY-PTP-$slice-live-gesture-target.out.txt"
$targetError = Join-Path $evidenceRoot "CAPY-PTP-$slice-live-gesture-target.err.txt"
$targetProcess = $null

Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;

namespace CapyIO.LiveTouchpadShellAcceptance {
    public static class Window {
        [DllImport("user32.dll")]
        public static extern System.IntPtr GetForegroundWindow();

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetForegroundWindow(System.IntPtr window);
    }
}
'@

function Get-CurrentVirtualDesktopId {
    $path = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops'
    $properties = Get-ItemProperty -LiteralPath $path -Name CurrentVirtualDesktop `
        -ErrorAction SilentlyContinue
    if ($null -eq $properties -or $null -eq $properties.CurrentVirtualDesktop) {
        return 'unavailable'
    }
    return (($properties.CurrentVirtualDesktop |
        ForEach-Object { $_.ToString('X2') }) -join '')
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Live Android VHF Shell acceptance requires an elevated administrator PowerShell.'
}
foreach ($required in @($executable, $targetScript)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Exact acceptance input is absent: $required"
    }
}
$actualHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash
if ($actualHash -ne $expectedHash) {
    throw "Acceptance executable hash mismatch: $actualHash"
}

New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Start-Transcript -LiteralPath $evidencePath -Force
try {
    Remove-Item -LiteralPath $targetOutput, $targetError -Force -ErrorAction SilentlyContinue
    $targetArguments = @(
        '-NoProfile', '-Sta', '-ExecutionPolicy', 'Bypass',
        '-File', ('"' + $targetScript + '"'), '-TimeoutSeconds', '120'
    )
    $targetProcess = Start-Process `
        -FilePath 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' `
        -ArgumentList $targetArguments `
        -RedirectStandardOutput $targetOutput `
        -RedirectStandardError $targetError `
        -PassThru
    Start-Sleep -Milliseconds 1200
    if ($targetProcess.HasExited) {
        throw 'The isolated Shell-gesture target exited before live input.'
    }

    $targetWindow = [IntPtr]::Zero
    for ($attempt = 0; $attempt -lt 20 -and $targetWindow -eq [IntPtr]::Zero; $attempt += 1) {
        $targetProcess.Refresh()
        $targetWindow = $targetProcess.MainWindowHandle
        if ($targetWindow -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 100 }
    }
    if ($targetWindow -eq [IntPtr]::Zero) {
        throw 'The isolated Shell-gesture target window was not found.'
    }

    $shell = New-Object -ComObject WScript.Shell
    $foregroundBefore = [IntPtr]::Zero
    for ($attempt = 0; $attempt -lt 20 -and $foregroundBefore -ne $targetWindow; $attempt += 1) {
        [void]$shell.AppActivate($targetProcess.Id)
        [void][CapyIO.LiveTouchpadShellAcceptance.Window]::SetForegroundWindow($targetWindow)
        Start-Sleep -Milliseconds 100
        $foregroundBefore = [CapyIO.LiveTouchpadShellAcceptance.Window]::GetForegroundWindow()
    }
    if ($foregroundBefore -ne $targetWindow) {
        throw "The isolated target is not foreground: target=$targetWindow foreground=$foregroundBefore"
    }

    $desktopBefore = Get-CurrentVirtualDesktopId
    Write-Output "acceptance=CAPY-PTP-$slice"
    Write-Output "gesture=$Gesture"
    Write-Output "required_contacts=$requiredContacts"
    Write-Output "executable_sha256=$actualHash"
    Write-Output 'projection=android-adb-reverse-to-installed-vhf'
    Write-Output 'restart_authorized=false'
    Write-Output 'driver_or_apk_installation_performed=false'
    Write-Output "target_window=$targetWindow"
    Write-Output "foreground_before=$foregroundBefore"
    Write-Output "virtual_desktop_before=$desktopBefore"

    $labLines = [System.Collections.Generic.List[string]]::new()
    $releaseThreshold = "--exit-after-release-at-least=$requiredContacts"
    & $executable `
        --inject `
        --acknowledge-desktop-input `
        --vhf `
        $releaseThreshold `
        --manual-session 2>&1 |
        ForEach-Object {
            $line = $_.ToString()
            $labLines.Add($line)
            Write-Output $line
        }
    $labExit = $LASTEXITCODE
    if ($labExit -ne 0) {
        throw "Live Android-to-VHF $Gesture-finger submission failed: $labExit"
    }

    $labText = $labLines -join [Environment]::NewLine
    $maxMatch = [regex]::Match($labText, '(?m)^max_contacts_observed=([0-9]+)\s*$')
    if (-not $maxMatch.Success) {
        throw 'Live receiver output did not report max_contacts_observed.'
    }
    $maxContacts = [int]$maxMatch.Groups[1].Value
    if ($maxContacts -ne $requiredContacts) {
        throw "Expected exactly $requiredContacts contacts but observed $maxContacts."
    }

    Start-Sleep -Milliseconds 1200
    $foregroundAfter = [CapyIO.LiveTouchpadShellAcceptance.Window]::GetForegroundWindow()
    $desktopAfter = Get-CurrentVirtualDesktopId
    $foregroundChanged = $foregroundAfter -ne $foregroundBefore
    $desktopChanged = $desktopBefore -ne 'unavailable' -and
        $desktopAfter -ne 'unavailable' -and $desktopBefore -ne $desktopAfter
    Write-Output "foreground_after=$foregroundAfter"
    Write-Output "virtual_desktop_after=$desktopAfter"
    Write-Output "foreground_changed=$($foregroundChanged.ToString().ToLowerInvariant())"
    Write-Output "virtual_desktop_changed=$($desktopChanged.ToString().ToLowerInvariant())"

    [void]$targetProcess.CloseMainWindow()
    if (-not $targetProcess.WaitForExit(3000)) {
        Stop-Process -Id $targetProcess.Id -Force
    }
    $targetText = Get-Content -LiteralPath $targetOutput -Raw -ErrorAction SilentlyContinue
    $targetErrors = Get-Content -LiteralPath $targetError -Raw -ErrorAction SilentlyContinue
    Write-Output $targetText
    if (-not [string]::IsNullOrWhiteSpace($targetErrors)) { Write-Output $targetErrors }

    if (-not $foregroundChanged -and -not $desktopChanged) {
        throw "Live VHF $Gesture-finger frames were accepted but no Shell state change was observed."
    }
    Write-Output "CAPY-PTP-$slice live Android VHF $Gesture-finger Shell acceptance: PASS"
}
catch {
    Write-Output "CAPY-PTP-$slice failure: $($_.Exception.Message)"
    throw
}
finally {
    if ($null -ne $targetProcess -and -not $targetProcess.HasExited) {
        Stop-Process -Id $targetProcess.Id -Force -ErrorAction SilentlyContinue
    }
    Stop-Transcript
}
