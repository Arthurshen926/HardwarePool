[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\deps\touchpad_runtime_worker-1ef346b5542e36f7.exe'
$expectedHash = 'F7F16EA7F903D1721D2D441B90B5816C19981A94DFA1A242C2C76985F5DD0A7C'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003J-vhf-one-finger.txt'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace CapyIO.TouchpadAcceptance003J {
    [StructLayout(LayoutKind.Sequential)]
    public struct Point {
        public int X;
        public int Y;
    }

    public static class Cursor {
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool GetCursorPos(out Point point);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetCursorPos(int x, int y);

        [DllImport("user32.dll")]
        public static extern int GetSystemMetrics(int index);
    }
}
'@

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'VHF desktop-input acceptance requires an elevated administrator PowerShell.'
}
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "Exact acceptance executable is absent: $executable"
}
$actualHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash
if ($actualHash -ne $expectedHash) {
    throw "Acceptance executable hash mismatch: $actualHash"
}

New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Start-Transcript -LiteralPath $evidencePath -Force
try {
    $virtualLeft = [CapyIO.TouchpadAcceptance003J.Cursor]::GetSystemMetrics(76)
    $virtualTop = [CapyIO.TouchpadAcceptance003J.Cursor]::GetSystemMetrics(77)
    $virtualWidth = [CapyIO.TouchpadAcceptance003J.Cursor]::GetSystemMetrics(78)
    $virtualHeight = [CapyIO.TouchpadAcceptance003J.Cursor]::GetSystemMetrics(79)
    $anchorX = $virtualLeft + [int]($virtualWidth / 2)
    $anchorY = $virtualTop + [int]($virtualHeight / 2)
    if (-not [CapyIO.TouchpadAcceptance003J.Cursor]::SetCursorPos($anchorX, $anchorY)) {
        throw "SetCursorPos failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    Start-Sleep -Milliseconds 100
    $before = New-Object CapyIO.TouchpadAcceptance003J.Point
    if (-not [CapyIO.TouchpadAcceptance003J.Cursor]::GetCursorPos([ref]$before)) {
        throw "GetCursorPos(before) failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    Write-Output "cursor_before=$($before.X),$($before.Y)"

    & $executable `
        authorized_vhf_worker_submits_one_finger_motion_then_releases_and_closes `
        --ignored --exact --nocapture
    $testExit = $LASTEXITCODE
    if ($testExit -ne 0) {
        throw "Exact VHF one-finger acceptance failed: $testExit"
    }
    Start-Sleep -Milliseconds 250
    $after = New-Object CapyIO.TouchpadAcceptance003J.Point
    if (-not [CapyIO.TouchpadAcceptance003J.Cursor]::GetCursorPos([ref]$after)) {
        throw "GetCursorPos(after) failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $deltaX = $after.X - $before.X
    $deltaY = $after.Y - $before.Y
    Write-Output "cursor_after=$($after.X),$($after.Y)"
    Write-Output "cursor_delta=$deltaX,$deltaY"
    if ($deltaX -eq 0 -and $deltaY -eq 0) {
        throw 'VHF frames were acknowledged but the Windows desktop cursor did not move.'
    }
    Write-Output 'CAPY-PTP-003J exact VHF one-finger acceptance: PASS; frames_submitted=4; clicks_submitted=0; cursor_moved=true'
}
finally {
    Stop-Transcript
}
