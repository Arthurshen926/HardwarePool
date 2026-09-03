[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\deps\touchpad_runtime_worker-1ef346b5542e36f7.exe'
$expectedHash = 'F7F16EA7F903D1721D2D441B90B5816C19981A94DFA1A242C2C76985F5DD0A7C'
$targetScript = Join-Path $PSScriptRoot 'windows-touchpad-click-target.ps1'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003K-vhf-tap.txt'
$targetOutput = Join-Path $evidenceRoot 'CAPY-PTP-003K-click-target.out.txt'
$targetError = Join-Path $evidenceRoot 'CAPY-PTP-003K-click-target.err.txt'
$targetProcess = $null

Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;

namespace CapyIO.TouchpadAcceptance003K {
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    public static class Cursor {
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetCursorPos(int x, int y);

        [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern System.IntPtr FindWindow(string className, string windowName);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool GetWindowRect(System.IntPtr window, out Rect rect);

        [DllImport("user32.dll")]
        public static extern System.IntPtr GetForegroundWindow();

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetForegroundWindow(System.IntPtr window);
    }
}
'@

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'VHF tap acceptance requires an elevated administrator PowerShell.'
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
        '-NoProfile',
        '-Sta',
        '-ExecutionPolicy', 'Bypass',
        '-File', ('"' + $targetScript + '"'),
        '-TimeoutSeconds', '20',
        '-CloseAfterFirstClick'
    )
    $targetProcess = Start-Process `
        -FilePath 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' `
        -ArgumentList $targetArguments `
        -RedirectStandardOutput $targetOutput `
        -RedirectStandardError $targetError `
        -PassThru
    Start-Sleep -Milliseconds 1200
    if ($targetProcess.HasExited) {
        throw 'The isolated click target exited before VHF input.'
    }

    $targetWindow = [IntPtr]::Zero
    for ($attempt = 0; $attempt -lt 20 -and $targetWindow -eq [IntPtr]::Zero; $attempt += 1) {
        $targetProcess.Refresh()
        $targetWindow = $targetProcess.MainWindowHandle
        if ($targetWindow -eq [IntPtr]::Zero) {
            $targetWindow = [CapyIO.TouchpadAcceptance003K.Cursor]::FindWindow(
                $null,
                'CapyIO Touchpad Click Target')
        }
        if ($targetWindow -eq [IntPtr]::Zero) {
            Start-Sleep -Milliseconds 100
        }
    }
    if ($targetWindow -eq [IntPtr]::Zero) {
        throw 'The isolated click-target window was not found.'
    }
    $targetRect = New-Object CapyIO.TouchpadAcceptance003K.Rect
    if (-not [CapyIO.TouchpadAcceptance003K.Cursor]::GetWindowRect($targetWindow, [ref]$targetRect)) {
        throw "GetWindowRect failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $shell = New-Object -ComObject WScript.Shell
    $foregroundWindow = [IntPtr]::Zero
    for ($attempt = 0; $attempt -lt 20 -and $foregroundWindow -ne $targetWindow; $attempt += 1) {
        [void]$shell.AppActivate($targetProcess.Id)
        [void][CapyIO.TouchpadAcceptance003K.Cursor]::SetForegroundWindow($targetWindow)
        Start-Sleep -Milliseconds 100
        $foregroundWindow = [CapyIO.TouchpadAcceptance003K.Cursor]::GetForegroundWindow()
    }
    if ($foregroundWindow -ne $targetWindow) {
        throw "The isolated click target is not foreground: target=$targetWindow foreground=$foregroundWindow"
    }
    $anchorX = $targetRect.Left + [int](($targetRect.Right - $targetRect.Left) / 2)
    $anchorY = $targetRect.Top + [int](($targetRect.Bottom - $targetRect.Top) / 2)
    if (-not [CapyIO.TouchpadAcceptance003K.Cursor]::SetCursorPos($anchorX, $anchorY)) {
        throw "SetCursorPos failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    Write-Output "target_window=$targetWindow"
    Write-Output "target_rect=$($targetRect.Left),$($targetRect.Top),$($targetRect.Right),$($targetRect.Bottom)"
    Write-Output "foreground_window=$foregroundWindow"
    Write-Output "cursor_anchor=$anchorX,$anchorY"

    & $executable `
        authorized_vhf_worker_submits_one_finger_tap_then_closes `
        --ignored --exact --nocapture
    $testExit = $LASTEXITCODE
    if ($testExit -ne 0) {
        throw "Exact VHF tap submission failed: $testExit"
    }

    if (-not $targetProcess.WaitForExit(5000)) {
        throw 'The VHF tap was acknowledged but the isolated target received no click.'
    }
    $targetText = Get-Content -LiteralPath $targetOutput -Raw -ErrorAction SilentlyContinue
    $targetErrors = Get-Content -LiteralPath $targetError -Raw -ErrorAction SilentlyContinue
    Write-Output $targetText
    if (-not [string]::IsNullOrWhiteSpace($targetErrors)) {
        Write-Output $targetErrors
    }
    if ($targetText -notmatch '(?m)^click_target_total=1\s*$') {
        throw 'The isolated target did not record exactly one Windows click.'
    }
    Write-Output 'CAPY-PTP-003K exact VHF tap acceptance: PASS; frames_submitted=4; click_observed=1'
}
catch {
    Write-Output "CAPY-PTP-003K failure: $($_.Exception.Message)"
    throw
}
finally {
    if ($null -ne $targetProcess -and -not $targetProcess.HasExited) {
        Stop-Process -Id $targetProcess.Id -Force -ErrorAction SilentlyContinue
    }
    Stop-Transcript
}
