[CmdletBinding()]
param(
    [switch]$ReuseSecondContactId,
    [switch]$AndroidCadence
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\deps\touchpad_runtime_worker-1ef346b5542e36f7.exe'
$expectedHash = '9A84830707BDA834C5463F5D46E5FE98C023C4A871F20B3AB987738F6117B468'
$targetScript = Join-Path $PSScriptRoot 'windows-touchpad-drag-target.ps1'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidenceName = if ($AndroidCadence) {
    'CAPY-PTP-0041-vhf-double-tap-drag-android-cadence.txt'
} elseif ($ReuseSecondContactId) {
    'CAPY-PTP-0041-vhf-double-tap-drag-reused-contact-id.txt'
} else {
    'CAPY-PTP-0041-vhf-double-tap-drag.txt'
}
$evidencePath = Join-Path $evidenceRoot $evidenceName
$targetOutput = Join-Path $evidenceRoot 'CAPY-PTP-0041-drag-target.out.txt'
$targetError = Join-Path $evidenceRoot 'CAPY-PTP-0041-drag-target.err.txt'
$targetProcess = $null

if ($ReuseSecondContactId -and $AndroidCadence) {
    throw 'Select at most one double-tap-and-drag diagnostic variant.'
}

Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;

namespace CapyIO.TouchpadAcceptance0041 {
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
    throw 'VHF double-tap-and-drag acceptance requires elevated PowerShell.'
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
        '-File', ('"' + $targetScript + '"'),
        '-TimeoutSeconds', '20', '-CloseAfterCompletedDrag'
    )
    $targetProcess = Start-Process `
        -FilePath 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' `
        -ArgumentList $targetArguments `
        -RedirectStandardOutput $targetOutput `
        -RedirectStandardError $targetError `
        -PassThru
    Start-Sleep -Milliseconds 1200
    if ($targetProcess.HasExited) {
        throw 'The isolated drag target exited before VHF input.'
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
        throw 'The isolated drag-target window was not found.'
    }
    $targetRect = New-Object CapyIO.TouchpadAcceptance0041.Rect
    if (-not [CapyIO.TouchpadAcceptance0041.Window]::GetWindowRect($targetWindow, [ref]$targetRect)) {
        throw "GetWindowRect failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $shell = New-Object -ComObject WScript.Shell
    $foregroundWindow = [IntPtr]::Zero
    for ($attempt = 0; $attempt -lt 20 -and $foregroundWindow -ne $targetWindow; $attempt += 1) {
        [void]$shell.AppActivate($targetProcess.Id)
        [void][CapyIO.TouchpadAcceptance0041.Window]::SetForegroundWindow($targetWindow)
        Start-Sleep -Milliseconds 100
        $foregroundWindow = [CapyIO.TouchpadAcceptance0041.Window]::GetForegroundWindow()
    }
    if ($foregroundWindow -ne $targetWindow) {
        throw "The isolated drag target is not foreground: target=$targetWindow foreground=$foregroundWindow"
    }
    $anchorX = $targetRect.Left + [int](($targetRect.Right - $targetRect.Left) / 3)
    $anchorY = $targetRect.Top + [int](($targetRect.Bottom - $targetRect.Top) / 2)
    if (-not [CapyIO.TouchpadAcceptance0041.Window]::SetCursorPos($anchorX, $anchorY)) {
        throw "SetCursorPos failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    Write-Output 'acceptance=CAPY-PTP-0041'
    Write-Output "reuse_second_contact_id=$($ReuseSecondContactId.IsPresent)"
    Write-Output "android_cadence=$($AndroidCadence.IsPresent)"
    Write-Output 'windows_tap_and_drag_enabled=true'
    Write-Output "executable_sha256=$actualHash"
    Write-Output "target_window=$targetWindow"
    Write-Output "target_rect=$($targetRect.Left),$($targetRect.Top),$($targetRect.Right),$($targetRect.Bottom)"
    Write-Output "foreground_window=$foregroundWindow"
    Write-Output "cursor_anchor=$anchorX,$anchorY"
    Write-Output 'driver_or_boot_change_performed=false'

    $testName = if ($AndroidCadence) {
        'authorized_vhf_worker_submits_android_cadence_double_tap_drag'
    } elseif ($ReuseSecondContactId) {
        'authorized_vhf_worker_submits_double_tap_drag_with_reused_contact_id'
    } else {
        'authorized_vhf_worker_submits_double_tap_drag_then_releases_and_closes'
    }
    & $executable `
        $testName `
        --ignored --exact --nocapture
    $testExit = $LASTEXITCODE
    if ($testExit -ne 0) {
        throw "Exact VHF double-tap-and-drag submission failed: $testExit"
    }

    if (-not $targetProcess.WaitForExit(22000)) {
        throw 'The isolated VHF drag target exceeded its bounded timeout.'
    }
    $targetText = Get-Content -LiteralPath $targetOutput -Raw -ErrorAction SilentlyContinue
    $targetErrors = Get-Content -LiteralPath $targetError -Raw -ErrorAction SilentlyContinue
    Write-Output $targetText
    if (-not [string]::IsNullOrWhiteSpace($targetErrors)) {
        Write-Output $targetErrors
    }
    if ($targetText -notmatch '(?m)^drag_target_completed=True\s*$') {
        throw 'The isolated target did not record a complete held primary-button drag.'
    }
    Write-Output 'CAPY-PTP-0041 fixed VHF double-tap-and-drag: PASS'
}
catch {
    Write-Output "CAPY-PTP-0041 failure: $($_.Exception.Message)"
    throw
}
finally {
    if ($null -ne $targetProcess -and -not $targetProcess.HasExited) {
        Stop-Process -Id $targetProcess.Id -Force -ErrorAction SilentlyContinue
    }
    Stop-Transcript
}
