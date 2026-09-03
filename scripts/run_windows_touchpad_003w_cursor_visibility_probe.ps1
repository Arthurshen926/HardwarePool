[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003W-cursor-visibility-probe.txt'

Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

namespace CapyIO.TouchpadAcceptance003W {
    [StructLayout(LayoutKind.Sequential)]
    public struct Point {
        public int X;
        public int Y;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct CursorInfo {
        public int Size;
        public int Flags;
        public IntPtr Cursor;
        public Point Position;
    }

    public static class CursorProbe {
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetCursorInfo(ref CursorInfo info);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetCursorPos(int x, int y);

        [DllImport("user32.dll")]
        private static extern int GetSystemMetrics(int index);

        [DllImport("user32.dll")]
        private static extern void mouse_event(
            uint flags,
            uint dx,
            uint dy,
            uint data,
            UIntPtr extraInfo);

        public static CursorInfo Read() {
            var info = new CursorInfo { Size = Marshal.SizeOf<CursorInfo>() };
            if (!GetCursorInfo(ref info)) {
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            return info;
        }

        public static Point VirtualDesktopCenter() {
            const int SmXVirtualScreen = 76;
            const int SmYVirtualScreen = 77;
            const int SmCxVirtualScreen = 78;
            const int SmCyVirtualScreen = 79;
            return new Point {
                X = GetSystemMetrics(SmXVirtualScreen) +
                    GetSystemMetrics(SmCxVirtualScreen) / 2,
                Y = GetSystemMetrics(SmYVirtualScreen) +
                    GetSystemMetrics(SmCyVirtualScreen) / 2,
            };
        }

        public static void WakeWithOnePixelMove() {
            const uint MouseEventMove = 0x0001;
            mouse_event(MouseEventMove, 1, 0, 0, UIntPtr.Zero);
        }
    }
}
'@

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The cursor visibility probe requires elevated interactive PowerShell.'
}

New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Start-Transcript -LiteralPath $evidencePath -Force
try {
    Write-Output 'acceptance=CAPY-PTP-003W'
    Write-Output 'desktop_input=one-pixel-relative-mouse-wake'
    Write-Output 'restart_authorized=false'
    Write-Output 'driver_or_apk_installation_performed=false'
    $before = [CapyIO.TouchpadAcceptance003W.CursorProbe]::Read()
    Write-Output "cursor_before=$($before.Position.X),$($before.Position.Y)"
    Write-Output "cursor_visible_before=$($before.Flags -band 1)"
    Write-Output "cursor_handle_before=$($before.Cursor)"
    $center = [CapyIO.TouchpadAcceptance003W.CursorProbe]::VirtualDesktopCenter()
    if (-not [CapyIO.TouchpadAcceptance003W.CursorProbe]::SetCursorPos($center.X, $center.Y)) {
        throw 'SetCursorPos failed.'
    }
    [CapyIO.TouchpadAcceptance003W.CursorProbe]::WakeWithOnePixelMove()
    Start-Sleep -Milliseconds 250
    $after = [CapyIO.TouchpadAcceptance003W.CursorProbe]::Read()
    Write-Output "cursor_after=$($after.Position.X),$($after.Position.Y)"
    Write-Output "cursor_visible_after=$($after.Flags -band 1)"
    Write-Output "cursor_handle_after=$($after.Cursor)"
    if (($after.Flags -band 1) -eq 0 -or $after.Cursor -eq [IntPtr]::Zero) {
        throw 'The relative mouse wake did not make the Windows cursor visible.'
    }
    Write-Output 'CAPY-PTP-003W cursor visibility wake: PASS'
}
finally {
    Stop-Transcript
}
