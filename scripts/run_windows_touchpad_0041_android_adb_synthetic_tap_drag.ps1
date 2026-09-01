[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\capyio-ptp-adb-lab.exe'
$expectedHash = 'D7C49E4111E71F1541F08485F334485851E574B29C4B7C8C6392422DA50233D3'
$targetScript = Join-Path $PSScriptRoot 'windows-touchpad-drag-target.ps1'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-0041-android-adb-synthetic-tap-drag.txt'
$targetOutput = Join-Path $evidenceRoot 'CAPY-PTP-0041-android-drag-target.out.txt'
$targetError = Join-Path $evidenceRoot 'CAPY-PTP-0041-android-drag-target.err.txt'
$labOutputPath = Join-Path $evidenceRoot 'CAPY-PTP-0041-android-receiver.out.txt'
$labErrorPath = Join-Path $evidenceRoot 'CAPY-PTP-0041-android-receiver.err.txt'
$targetProcess = $null
$labProcess = $null

Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;

namespace CapyIO.TouchpadAcceptance0041Android {
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    public static class Window {
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool GetWindowRect(System.IntPtr window, out Rect rect);

        [DllImport("user32.dll")]
        public static extern System.IntPtr GetForegroundWindow();

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetForegroundWindow(System.IntPtr window);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetCursorPos(int x, int y);
    }
}
'@

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Android end-to-end tap-and-drag acceptance requires elevated PowerShell.'
}
foreach ($required in @($executable, $targetScript)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Exact acceptance input is absent: $required"
    }
}
$actualHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash
if ($actualHash -ne $expectedHash) {
    throw "Android lab executable hash mismatch: $actualHash"
}

New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Start-Transcript -LiteralPath $evidencePath -Force
try {
    Remove-Item -LiteralPath $targetOutput, $targetError, $labOutputPath, $labErrorPath -Force -ErrorAction SilentlyContinue
    $targetArguments = @(
        '-NoProfile', '-Sta', '-ExecutionPolicy', 'Bypass',
        '-File', ('"' + $targetScript + '"'),
        '-TimeoutSeconds', '30', '-CloseAfterCompletedDrag'
    )
    $targetProcess = Start-Process `
        -FilePath 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' `
        -ArgumentList $targetArguments `
        -RedirectStandardOutput $targetOutput `
        -RedirectStandardError $targetError `
        -PassThru
    Start-Sleep -Milliseconds 1200
    if ($targetProcess.HasExited) {
        throw 'The isolated Android drag target exited before input.'
    }

    $targetWindow = [IntPtr]::Zero
    for ($attempt = 0; $attempt -lt 20 -and $targetWindow -eq [IntPtr]::Zero; $attempt += 1) {
        $targetProcess.Refresh()
        $targetWindow = $targetProcess.MainWindowHandle
        if ($targetWindow -eq [IntPtr]::Zero) {
            Start-Sleep -Milliseconds 100
        }
    }
    if ($targetWindow -eq [IntPtr]::Zero) {
        throw 'The isolated Android drag-target window was not found.'
    }
    $targetRect = New-Object CapyIO.TouchpadAcceptance0041Android.Rect
    if (-not [CapyIO.TouchpadAcceptance0041Android.Window]::GetWindowRect($targetWindow, [ref]$targetRect)) {
        throw "GetWindowRect failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $shell = New-Object -ComObject WScript.Shell
    $foregroundWindow = [IntPtr]::Zero
    for ($attempt = 0; $attempt -lt 20 -and $foregroundWindow -ne $targetWindow; $attempt += 1) {
        [void]$shell.AppActivate($targetProcess.Id)
        [void][CapyIO.TouchpadAcceptance0041Android.Window]::SetForegroundWindow($targetWindow)
        Start-Sleep -Milliseconds 100
        $foregroundWindow = [CapyIO.TouchpadAcceptance0041Android.Window]::GetForegroundWindow()
    }
    if ($foregroundWindow -ne $targetWindow) {
        throw "The isolated Android drag target is not foreground: target=$targetWindow foreground=$foregroundWindow"
    }
    $anchorX = $targetRect.Left + [int](($targetRect.Right - $targetRect.Left) / 3)
    $anchorY = $targetRect.Top + [int](($targetRect.Bottom - $targetRect.Top) / 2)
    if (-not [CapyIO.TouchpadAcceptance0041Android.Window]::SetCursorPos($anchorX, $anchorY)) {
        throw "SetCursorPos failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    Write-Output 'acceptance=CAPY-PTP-0041-android-adb-synthetic'
    Write-Output 'source=android-shell-input-through-activity-jni-transport-vhf'
    Write-Output 'physical_touch_claim=false'
    Write-Output 'windows_tap_and_drag_enabled=true'
    Write-Output "executable_sha256=$actualHash"
    Write-Output "target_window=$targetWindow"
    Write-Output "target_rect=$($targetRect.Left),$($targetRect.Top),$($targetRect.Right),$($targetRect.Bottom)"
    Write-Output "foreground_window=$foregroundWindow"
    Write-Output "cursor_anchor=$anchorX,$anchorY"
    Write-Output 'driver_or_boot_change_performed=false'

    $labProcess = Start-Process `
        -FilePath $executable `
        -ArgumentList @(
            '--inject',
            '--acknowledge-desktop-input',
            '--vhf',
            '--trace-tap-drag'
        ) `
        -RedirectStandardOutput $labOutputPath `
        -RedirectStandardError $labErrorPath `
        -PassThru `
        -NoNewWindow
    if (-not $labProcess.WaitForExit(190000)) {
        throw 'Android end-to-end receiver exceeded its bounded 190-second process timeout.'
    }
    $labProcess.WaitForExit()
    $labProcess.Refresh()
    $labExit = $labProcess.ExitCode
    $labOutput = @(Get-Content -LiteralPath $labOutputPath -ErrorAction SilentlyContinue)
    $labErrors = @(Get-Content -LiteralPath $labErrorPath -ErrorAction SilentlyContinue)
    $labOutput | Write-Output
    $labErrors | Write-Output
    $labText = $labOutput -join [Environment]::NewLine
    if ($labText -notmatch '(?m)^tap_drag_trace_complete=true\s*$') {
        throw 'Android end-to-end receiver did not capture two complete gestures.'
    }
    if ($null -eq $labExit) {
        if ($labErrors.Count -gt 0) {
            throw 'Android receiver exit code was unavailable and stderr was not empty.'
        }
        Write-Output 'receiver_exit_code=unavailable_trace_complete'
    } elseif ($labExit -ne 0) {
        throw "Android end-to-end tap-and-drag receiver failed: $labExit"
    } else {
        Write-Output 'receiver_exit_code=0'
    }

    if (-not $targetProcess.WaitForExit(32000)) {
        throw 'The isolated Android/VHF drag target exceeded its bounded timeout.'
    }
    $targetText = Get-Content -LiteralPath $targetOutput -Raw -ErrorAction SilentlyContinue
    $targetErrors = Get-Content -LiteralPath $targetError -Raw -ErrorAction SilentlyContinue
    Write-Output $targetText
    if (-not [string]::IsNullOrWhiteSpace($targetErrors)) {
        Write-Output $targetErrors
    }
    if ($targetText -notmatch '(?m)^drag_target_completed=True\s*$') {
        throw 'The isolated target did not record an Android-originated held drag.'
    }
    Write-Output 'CAPY-PTP-0041 Android ADB-synthetic end-to-end tap-and-drag: PASS'
}
catch {
    Write-Output "CAPY-PTP-0041 Android end-to-end failure: $($_.Exception.Message)"
    throw
}
finally {
    if ($null -ne $targetProcess -and -not $targetProcess.HasExited) {
        Stop-Process -Id $targetProcess.Id -Force -ErrorAction SilentlyContinue
    }
    if ($null -ne $labProcess -and -not $labProcess.HasExited) {
        Stop-Process -Id $labProcess.Id -Force -ErrorAction SilentlyContinue
    }
    Stop-Transcript
}
