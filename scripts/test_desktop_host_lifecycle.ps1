param([Parameter(Mandatory = $true)][string]$Executable)
$ErrorActionPreference = 'Stop'
$binary = (Resolve-Path -LiteralPath $Executable).Path
if (Get-Process capyio-desktop -ErrorAction SilentlyContinue) {
    throw 'Close the existing CapyIO node before this controlled software check.'
}
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class CapyWindowCheck {
    public delegate bool EnumProc(IntPtr h, IntPtr p);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr p);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder t, int n);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, UIntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern IntPtr GetMenu(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr GetSubMenu(IntPtr h, int i);
    [DllImport("user32.dll")] static extern int GetMenuItemCount(IntPtr h);
    [DllImport("user32.dll")] static extern uint GetMenuItemID(IntPtr h, int i);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetMenuString(IntPtr h, uint i, StringBuilder s, int n, uint f);
    public static IntPtr Find(uint pid) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((h,p) => {
            uint owner; GetWindowThreadProcessId(h, out owner);
            var title = new StringBuilder(256); GetWindowText(h, title, 256);
            if (owner == pid && title.ToString() == "CapyIO") { result=h; return false; }
            return true;
        }, IntPtr.Zero);
        return result;
    }
    public static uint QuitItem(IntPtr menu) {
        for(int i=0; i<GetMenuItemCount(menu); i++) {
            var label=new StringBuilder(256); GetMenuString(menu,(uint)i,label,256,0x400);
            if(label.ToString().Contains("退出节点")) return GetMenuItemID(menu,i);
            var sub=GetSubMenu(menu,i);
            if(sub!=IntPtr.Zero) { var id=QuitItem(sub); if(id!=uint.MaxValue) return id; }
        }
        return uint.MaxValue;
    }
}
'@
function Wait-Window($Process, [bool]$Present) {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($Process.HasExited) { throw "Node exited unexpectedly: $($Process.ExitCode)" }
        $window = [CapyWindowCheck]::Find($Process.Id)
        if (($window -ne [IntPtr]::Zero) -eq $Present) { return $window }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Window presence did not become $Present"
}
function Invoke-NodeCommand([string]$Command, $Arguments = @{}, [int]$Version = 1) {
    $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $session = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
    $pipe = [System.IO.Pipes.NamedPipeClientStream]::new('.', "CapyIO.Node.v1.$sid.$session", [System.IO.Pipes.PipeDirection]::InOut, [System.IO.Pipes.PipeOptions]::Asynchronous)
    try {
        $pipe.Connect(5000)
        $payload = [Text.Encoding]::UTF8.GetBytes((@{version=$Version;command=$Command;args=$Arguments} | ConvertTo-Json -Compress -Depth 10))
        $writer = [System.IO.BinaryWriter]::new($pipe, [Text.Encoding]::UTF8, $true)
        $writer.Write([int]$payload.Length)
        $writer.Write($payload)
        $writer.Flush()
        $header = [byte[]]::new(4)
        Read-NodeBytes $pipe $header
        $length = [BitConverter]::ToInt32($header,0)
        if ($length -le 0 -or $length -gt 1048576) { throw 'Node response outside bounds' }
        $reply = [byte[]]::new($length)
        Read-NodeBytes $pipe $reply
        $pipe.WriteByte(1)
        $pipe.Flush()
        return [Text.Encoding]::UTF8.GetString($reply) | ConvertFrom-Json
    } finally { $pipe.Dispose() }
}
function Read-NodeBytes($Pipe, [byte[]]$Buffer) {
    $offset = 0
    while ($offset -lt $Buffer.Length) {
        $read = $Pipe.ReadAsync($Buffer, $offset, $Buffer.Length - $offset)
        if (-not $read.Wait(10000)) { throw 'Node response timeout' }
        if ($read.Result -eq 0) { throw 'Node response closed' }
        $offset += $read.Result
    }
}
$ui = Start-Process -FilePath $binary -PassThru -WindowStyle Hidden
$window = Wait-Window $ui $true
$initial = Invoke-NodeCommand 'host_status'
if (-not $initial.Ok -or $initial.Ok.pid -eq $ui.Id) { throw 'UI and Host are not separate processes' }
$hostProcess = Get-Process -Id $initial.Ok.pid
$snapshot = Invoke-NodeCommand 'get_snapshot'
if (-not $snapshot.Ok) { throw "Snapshot command failed: $($snapshot.Err)" }
if (-not (Invoke-NodeCommand 'unknown-command').Err) { throw 'Unknown command accepted' }
if (-not (Invoke-NodeCommand 'host_status' @{} 999).Err) { throw 'Invalid API version accepted' }
if (-not (Invoke-NodeCommand 'set_route' @{request=@{routeId='invalid';active=$true}}).Err) { throw 'Malformed Route accepted' }
Write-Output "STARTED ui_pid=$($ui.Id) host_pid=$($hostProcess.Id); snapshot and command boundary PASS"
if (-not [CapyWindowCheck]::PostMessage($window, 0x10, [UIntPtr]::Zero, [IntPtr]::Zero)) { throw 'WM_CLOSE failed' }
if (-not $ui.WaitForExit(10000) -or $ui.ExitCode -ne 0) { throw 'Closing window did not exit UI cleanly' }
if ($hostProcess.HasExited -or (Invoke-NodeCommand 'host_status').Ok.pid -ne $hostProcess.Id) { throw 'Closing UI stopped the Host' }
Write-Output "UI_EXITED host_alive=True host_pid=$($hostProcess.Id)"
$ui = Start-Process -FilePath $binary -PassThru -WindowStyle Hidden
$window = Wait-Window $ui $true
if ((Invoke-NodeCommand 'host_status').Ok.pid -ne $hostProcess.Id) { throw 'Reopen replaced the Host' }
Write-Output "REATTACHED ui_pid=$($ui.Id) same_host_pid=$($hostProcess.Id)"
# Only this script's idle UI is terminated; no physical Route is started.
Stop-Process -Id $ui.Id
$null = $ui.WaitForExit(10000)
if ($hostProcess.HasExited -or -not (Invoke-NodeCommand 'get_snapshot').Ok) { throw 'UI crash affected the Host' }
Write-Output 'UI_CRASH_ISOLATED snapshot=PASS'
$ui = Start-Process -FilePath $binary -PassThru -WindowStyle Hidden
$window = Wait-Window $ui $true
Start-Sleep -Milliseconds 500
$quit = [CapyWindowCheck]::QuitItem([CapyWindowCheck]::GetMenu($window))
if ($quit -eq [uint32]::MaxValue) { throw 'Explicit quit-node menu item missing' }
if (-not [CapyWindowCheck]::PostMessage($window, 0x111, [UIntPtr]::new($quit), [IntPtr]::Zero)) { throw 'Quit menu dispatch failed' }
if (-not $ui.WaitForExit(20000) -or $ui.ExitCode -ne 0) { throw 'Explicit UI exit failed' }
if (-not $hostProcess.WaitForExit(20000)) { throw 'Explicit Host exit failed' }
Write-Output "QUIT_NODE ui_exit=$($ui.ExitCode) host_exited=True"
Write-Output 'CAPYIO_DESKTOP_LIFECYCLE_PASSED (separate processes; no active physical Routes)'
