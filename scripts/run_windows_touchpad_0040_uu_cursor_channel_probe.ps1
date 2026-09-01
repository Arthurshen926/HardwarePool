[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-0040-uu-cursor-channel-probe.txt'

Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

namespace CapyIO.TouchpadAcceptance0040 {
    [StructLayout(LayoutKind.Sequential)]
    public struct Point {
        public int X;
        public int Y;
    }

    public static class CursorProbe {
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetCursorPos(out Point point);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool SetCursorPos(int x, int y);

        [DllImport("user32.dll")]
        private static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extraInfo);

        [DllImport("user32.dll")]
        public static extern int GetSystemMetrics(int index);

        public static Point Read() {
            Point point;
            if (!GetCursorPos(out point)) {
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            return point;
        }

        public static Point Set(int x, int y) {
            if (!SetCursorPos(x, y)) {
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            System.Threading.Thread.Sleep(100);
            return Read();
        }

        public static Point MoveRelative(int dx, int dy) {
            const uint MouseEventMove = 0x0001;
            mouse_event(MouseEventMove, dx, dy, 0, UIntPtr.Zero);
            System.Threading.Thread.Sleep(100);
            return Read();
        }
    }
}
'@

New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Start-Transcript -LiteralPath $evidencePath -Force
try {
    $original = [CapyIO.TouchpadAcceptance0040.CursorProbe]::Read()
    $left = [CapyIO.TouchpadAcceptance0040.CursorProbe]::GetSystemMetrics(76)
    $top = [CapyIO.TouchpadAcceptance0040.CursorProbe]::GetSystemMetrics(77)
    $width = [CapyIO.TouchpadAcceptance0040.CursorProbe]::GetSystemMetrics(78)
    $height = [CapyIO.TouchpadAcceptance0040.CursorProbe]::GetSystemMetrics(79)
    if ($width -le 0 -or $height -le 0) {
        throw "Invalid virtual desktop bounds: $left,$top,$width,$height"
    }

    Write-Output 'acceptance=CAPY-PTP-0040'
    Write-Output 'purpose=distinguish-UU-system-cursor-polling-from-mouse-event-tracking'
    Write-Output 'desktop_input_acknowledged=true'
    Write-Output "interactive_session_id=$((Get-Process -Id $PID).SessionId)"
    Write-Output 'restart_authorized=false'
    Write-Output "original_cursor=$($original.X),$($original.Y)"
    Write-Output "virtual_desktop=$left,$top,$width,$height"
    Write-Output 'countdown_seconds=5'
    Start-Sleep -Seconds 5

    $x25 = $left + [int]($width * 0.25)
    $x75 = $left + [int]($width * 0.75)
    $y25 = $top + [int]($height * 0.25)
    $y75 = $top + [int]($height * 0.75)
    $points = @(
        [pscustomobject]@{ X = $x25; Y = $y25 }
        [pscustomobject]@{ X = $x75; Y = $y25 }
        [pscustomobject]@{ X = $x75; Y = $y75 }
        [pscustomobject]@{ X = $x25; Y = $y75 }
    )
    Write-Output 'stage_1=set-cursor-position'
    foreach ($point in $points) {
        $actual = [CapyIO.TouchpadAcceptance0040.CursorProbe]::Set($point.X, $point.Y)
        Write-Output "stage_1_actual=$($actual.X),$($actual.Y)"
        Start-Sleep -Milliseconds 750
    }

    $centerX = $left + [int]($width / 2)
    $centerY = $top + [int]($height / 2)
    [void][CapyIO.TouchpadAcceptance0040.CursorProbe]::Set($centerX, $centerY)
    Write-Output 'inter_stage_pause_seconds=2'
    Start-Sleep -Seconds 2

    Write-Output 'stage_2=relative-mouse-event'
    $deltas = @(
        [pscustomobject]@{ X = 300; Y = 0 }
        [pscustomobject]@{ X = 0; Y = 220 }
        [pscustomobject]@{ X = -300; Y = 0 }
        [pscustomobject]@{ X = 0; Y = -220 }
    )
    foreach ($delta in $deltas) {
        $actual = [CapyIO.TouchpadAcceptance0040.CursorProbe]::MoveRelative(
            $delta.X,
            $delta.Y
        )
        Write-Output "stage_2_actual=$($actual.X),$($actual.Y)"
        Start-Sleep -Milliseconds 750
    }

    $restored = [CapyIO.TouchpadAcceptance0040.CursorProbe]::Set(
        $original.X,
        $original.Y
    )
    Write-Output "restored_cursor=$($restored.X),$($restored.Y)"
    Write-Output 'CAPY-PTP-0040 UU cursor-channel probe: COMPLETE'
}
catch {
    Write-Output "probe_failure=$($_.Exception.GetType().FullName):$($_.Exception.Message)"
    throw
}
finally {
    Stop-Transcript
}
