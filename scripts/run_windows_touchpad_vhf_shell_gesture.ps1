[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('three', 'four')]
    [string]$Gesture
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repositoryRoot 'target\debug\deps\touchpad_runtime_worker-1ef346b5542e36f7.exe'
$expectedHash = 'F7F16EA7F903D1721D2D441B90B5816C19981A94DFA1A242C2C76985F5DD0A7C'
$targetScript = Join-Path $PSScriptRoot 'windows-touchpad-gesture-target.ps1'
$slice = if ($Gesture -eq 'three') { '003M' } else { '003N' }
$testName = if ($Gesture -eq 'three') {
    'authorized_vhf_worker_submits_three_finger_swipe_then_releases_and_closes'
} else {
    'authorized_vhf_worker_submits_four_finger_swipe_then_releases_and_closes'
}
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot "CAPY-PTP-$slice-vhf-$Gesture-finger.txt"
$targetOutput = Join-Path $evidenceRoot "CAPY-PTP-$slice-gesture-target.out.txt"
$targetError = Join-Path $evidenceRoot "CAPY-PTP-$slice-gesture-target.err.txt"
$targetProcess = $null

Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;

namespace CapyIO.TouchpadShellAcceptance {
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
    throw 'VHF Shell-gesture acceptance requires an elevated administrator PowerShell.'
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
        '-File', ('"' + $targetScript + '"'), '-TimeoutSeconds', '20'
    )
    $targetProcess = Start-Process `
        -FilePath 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' `
        -ArgumentList $targetArguments `
        -RedirectStandardOutput $targetOutput `
        -RedirectStandardError $targetError `
        -PassThru
    Start-Sleep -Milliseconds 1200
    if ($targetProcess.HasExited) {
        throw 'The isolated Shell-gesture target exited before VHF input.'
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
        [void][CapyIO.TouchpadShellAcceptance.Window]::SetForegroundWindow($targetWindow)
        Start-Sleep -Milliseconds 100
        $foregroundBefore = [CapyIO.TouchpadShellAcceptance.Window]::GetForegroundWindow()
    }
    if ($foregroundBefore -ne $targetWindow) {
        throw "The isolated target is not foreground: target=$targetWindow foreground=$foregroundBefore"
    }
    $desktopBefore = Get-CurrentVirtualDesktopId
    Write-Output "gesture=$Gesture"
    Write-Output "target_window=$targetWindow"
    Write-Output "foreground_before=$foregroundBefore"
    Write-Output "virtual_desktop_before=$desktopBefore"

    & $executable $testName --ignored --exact --nocapture
    $testExit = $LASTEXITCODE
    if ($testExit -ne 0) {
        throw "Exact VHF $Gesture-finger submission failed: $testExit"
    }
    Start-Sleep -Milliseconds 1200
    $foregroundAfter = [CapyIO.TouchpadShellAcceptance.Window]::GetForegroundWindow()
    $desktopAfter = Get-CurrentVirtualDesktopId
    Write-Output "foreground_after=$foregroundAfter"
    Write-Output "virtual_desktop_after=$desktopAfter"

    [void]$targetProcess.CloseMainWindow()
    if (-not $targetProcess.WaitForExit(3000)) {
        Stop-Process -Id $targetProcess.Id -Force
    }
    $targetText = Get-Content -LiteralPath $targetOutput -Raw -ErrorAction SilentlyContinue
    $targetErrors = Get-Content -LiteralPath $targetError -Raw -ErrorAction SilentlyContinue
    Write-Output $targetText
    if (-not [string]::IsNullOrWhiteSpace($targetErrors)) { Write-Output $targetErrors }
    $deactivatedMatch = [regex]::Match(
        $targetText,
        '(?m)^gesture_target_deactivated=([0-9]+)\s*$')
    $deactivated = if ($deactivatedMatch.Success) {
        [int]$deactivatedMatch.Groups[1].Value
    } else {
        0
    }
    $desktopChanged = $desktopBefore -ne 'unavailable' -and
        $desktopAfter -ne 'unavailable' -and $desktopBefore -ne $desktopAfter
    $foregroundChanged = $foregroundAfter -ne $foregroundBefore
    Write-Output "foreground_changed=$($foregroundChanged.ToString().ToLowerInvariant())"
    Write-Output "virtual_desktop_changed=$($desktopChanged.ToString().ToLowerInvariant())"
    Write-Output "target_deactivated=$deactivated"
    if (-not $foregroundChanged -and -not $desktopChanged -and $deactivated -lt 1) {
        throw "VHF $Gesture-finger frames were acknowledged but no Shell state change was observed."
    }
    Write-Output "CAPY-PTP-$slice exact VHF $Gesture-finger acceptance: PASS; frames_submitted=11"
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
