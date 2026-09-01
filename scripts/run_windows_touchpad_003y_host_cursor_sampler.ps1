[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003Y-host-cursor-sampler.txt'

Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

namespace CapyIO.TouchpadAcceptance003Y {
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

        public static CursorInfo Read() {
            var info = new CursorInfo { Size = Marshal.SizeOf<CursorInfo>() };
            if (!GetCursorInfo(ref info)) {
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            return info;
        }
    }
}
'@

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The host cursor sampler requires elevated interactive PowerShell.'
}

New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Start-Transcript -LiteralPath $evidencePath -Force
try {
    Write-Output 'acceptance=CAPY-PTP-003Y'
    Write-Output 'sample_target=interactive-host-cursor-during-remote-viewing'
    Write-Output 'sample_duration_seconds=20'
    Write-Output 'sample_interval_milliseconds=20'
    Write-Output 'desktop_input_submitted=false'
    $sampleStart = Get-Date
    $minimumX = [int]::MaxValue
    $maximumX = [int]::MinValue
    $minimumY = [int]::MaxValue
    $maximumY = [int]::MinValue
    $firstPosition = $null
    $lastPosition = $null
    $positions = New-Object 'System.Collections.Generic.HashSet[string]'
    $cursorHandles = New-Object 'System.Collections.Generic.HashSet[string]'
    $hiddenSamples = 0
    $sampleCount = 0
    while (((Get-Date) - $sampleStart).TotalSeconds -lt 20) {
        $cursor = [CapyIO.TouchpadAcceptance003Y.CursorProbe]::Read()
        $position = "$($cursor.Position.X),$($cursor.Position.Y)"
        if ($null -eq $firstPosition) {
            $firstPosition = $position
        }
        $lastPosition = $position
        [void]$positions.Add($position)
        [void]$cursorHandles.Add($cursor.Cursor.ToString())
        if (($cursor.Flags -band 1) -eq 0) {
            $hiddenSamples += 1
        }
        $minimumX = [Math]::Min($minimumX, $cursor.Position.X)
        $maximumX = [Math]::Max($maximumX, $cursor.Position.X)
        $minimumY = [Math]::Min($minimumY, $cursor.Position.Y)
        $maximumY = [Math]::Max($maximumY, $cursor.Position.Y)
        $sampleCount += 1
        Start-Sleep -Milliseconds 20
    }
    Write-Output "samples=$sampleCount"
    Write-Output "cursor_first=$firstPosition"
    Write-Output "cursor_last=$lastPosition"
    Write-Output "cursor_unique_positions=$($positions.Count)"
    Write-Output "cursor_x_range=$minimumX..$maximumX"
    Write-Output "cursor_y_range=$minimumY..$maximumY"
    Write-Output "cursor_unique_handles=$($cursorHandles.Count)"
    Write-Output "cursor_hidden_samples=$hiddenSamples"
    Write-Output 'CAPY-PTP-003Y interactive host cursor sampling: COMPLETE'
}
finally {
    Stop-Transcript
}
