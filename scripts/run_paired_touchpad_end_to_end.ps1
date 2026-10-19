[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-fA-F-]{36}$')]
    [string]$NodeId,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$')]
    [string]$HostAddress,

    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 65535)]
    [int]$ControlPort,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^(?:[0-9]{1,3}\.){3}[0-9]{1,3}:[0-9]{1,5}$')]
    [string]$AdbSerial,

    [ValidateRange(1, 8)]
    [int]$SwipeCount = 1
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$desktop = Join-Path $repositoryRoot 'target\debug\capyio-desktop.exe'
$expectedHash = 'A96EBDE203F0E85E9AF59F321A12E1F015ABC44E38160AE40FE193A822CABDEE'
$adb = Join-Path $env:LOCALAPPDATA 'Android\Sdk\platform-tools\adb.exe'
$evaluator = Join-Path $repositoryRoot 'scripts\android_webview_cdp_eval.mjs'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-INTEGRATED-paired-touchpad-e2e.txt'
$gateOutput = Join-Path $evidenceRoot 'CAPY-INTEGRATED-paired-touchpad-e2e-gate.stdout.txt'
$gateError = Join-Path $evidenceRoot 'CAPY-INTEGRATED-paired-touchpad-e2e-gate.stderr.txt'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'End-to-end touchpad acceptance requires elevated PowerShell.'
}
if ((Get-FileHash -LiteralPath $desktop -Algorithm SHA256).Hash -ne $expectedHash) {
    throw 'CapyIO Desktop hash mismatch.'
}
if (-not (Test-Path -LiteralPath $adb -PathType Leaf)) {
    throw 'Android platform-tools adb is absent.'
}

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class CapyIOEndToEndCursor {
    [StructLayout(LayoutKind.Sequential)]
    public struct Point { public int X; public int Y; }
    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool GetCursorPos(out Point point);
}
'@

New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Start-Transcript -LiteralPath $evidencePath -Force
try {
    $gate = Start-Process -FilePath $desktop -ArgumentList @(
        '--verify-paired-touchpad-data',
        $NodeId,
        $HostAddress,
        $ControlPort,
        '--acknowledge-desktop-input'
    ) -WindowStyle Hidden -RedirectStandardOutput $gateOutput -RedirectStandardError $gateError -PassThru

    $active = $false
    $activeState = $null
    $approvedRoute = $null
    for ($attempt = 0; $attempt -lt 200; $attempt += 1) {
        if (-not $approvedRoute -and (Test-Path -LiteralPath $gateOutput)) {
            $gateText = Get-Content -LiteralPath $gateOutput -Raw
            if ($gateText -match 'CAPYIO_RESOURCE_CONSENT_PENDING route=([0-9a-fA-F-]{36})') {
                $approvedRoute = $Matches[1]
                $approvalResult = & node $evaluator 'http://127.0.0.1:9223/json/list' `
                    "window.CapyIOAndroid.resolveResourceRequest('$approvedRoute',true)" `
                    2>$null
                if ($approvalResult -notmatch 'true') {
                    throw "Android rejected exact Route approval: route=$approvedRoute result=$approvalResult"
                }
                Write-Output "approved_route=$approvedRoute"
            }
        }
        $activeState = & node $evaluator 'http://127.0.0.1:9223/json/list' `
            "JSON.stringify({touchpad:JSON.parse(window.CapyIOAndroid.getTouchpadStateJson()),main:document.querySelector('main')?.className,surface:document.querySelector('.touchpad-surface--immersive')!==null})" `
            2>$null
        if ($activeState -match '\\"status\\":\\"active\\"' -and $activeState -match '\\"surface\\":true') {
            $active = $true
            break
        }
        Start-Sleep -Milliseconds 150
    }
    if (-not $active) {
        throw 'Android touchpad did not enter active state.'
    }
    if (-not $approvedRoute) {
        throw 'The test did not observe and approve its exact pending Route.'
    }
    Write-Output "android_active_state=$activeState"

    $before = New-Object CapyIOEndToEndCursor+Point
    if (-not [CapyIOEndToEndCursor]::GetCursorPos([ref]$before)) {
        throw "GetCursorPos(before) failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $totalCursorTravel = 0
    $last = $before
    for ($swipe = 0; $swipe -lt $SwipeCount; $swipe += 1) {
        $fromX = if ($swipe % 2 -eq 0) { 1200 } else { 2200 }
        $toX = if ($swipe % 2 -eq 0) { 2200 } else { 1200 }
        & $adb -s $AdbSerial shell input swipe $fromX 850 $toX 850 450 | Out-Null
        if ($LASTEXITCODE -ne 0) {
            throw "ADB touch injection failed on swipe $($swipe + 1): $LASTEXITCODE"
        }
        $current = New-Object CapyIOEndToEndCursor+Point
        if (-not [CapyIOEndToEndCursor]::GetCursorPos([ref]$current)) {
            throw "GetCursorPos(swipe $($swipe + 1)) failed"
        }
        $travel = [Math]::Abs($current.X - $last.X) + [Math]::Abs($current.Y - $last.Y)
        $totalCursorTravel += $travel
        Write-Output ('swipe_{0}_cursor={1},{2} travel={3}' -f ($swipe + 1), $current.X, $current.Y, $travel)
        $last = $current
    }
    Start-Sleep -Milliseconds 800
    $after = New-Object CapyIOEndToEndCursor+Point
    if (-not [CapyIOEndToEndCursor]::GetCursorPos([ref]$after)) {
        throw "GetCursorPos(after) failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $deltaX = $after.X - $before.X
    $deltaY = $after.Y - $before.Y
    Write-Output "cursor_before=$($before.X),$($before.Y)"
    Write-Output "cursor_after=$($after.X),$($after.Y)"
    Write-Output "cursor_delta=$deltaX,$deltaY"
    Write-Output "cursor_total_travel=$totalCursorTravel"
    Write-Output "swipes_injected=$SwipeCount"
    if ($totalCursorTravel -eq 0 -and $deltaX -eq 0 -and $deltaY -eq 0) {
        throw 'End-to-end touchpad frames did not move the Windows cursor.'
    }
    if (-not $gate.WaitForExit(30000)) {
        throw 'Paired touchpad gate did not stop within its bound.'
    }
    # Drain redirected output. Windows PowerShell 5 can retain a stale
    # Start-Process ExitCode when both output streams are redirected, so the
    # gate's bounded, Route-specific success record is the acceptance signal.
    $gate.WaitForExit()
    $gateText = Get-Content -LiteralPath $gateOutput -Raw
    if ($gateText -notmatch "CAPYIO_TOUCHPAD_DATA_PASSED route=$([regex]::Escape($approvedRoute)) transport=mtls") {
        throw "Paired touchpad gate did not emit its Route-specific success record."
    }
    Write-Output 'CAPYIO_PAIRED_TOUCHPAD_END_TO_END_PASSED=true'
}
finally {
    if ($gate -and -not $gate.HasExited) {
        $gate.Kill()
        $gate.WaitForExit()
    }
    if (Test-Path -LiteralPath $gateOutput) { Get-Content -LiteralPath $gateOutput }
    if (Test-Path -LiteralPath $gateError) { Get-Content -LiteralPath $gateError }
    Stop-Transcript
}
