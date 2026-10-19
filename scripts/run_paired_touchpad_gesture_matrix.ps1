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

    [ValidateSet('all','single_tap','single_move','two_finger_tap','two_finger_scroll','three_finger_swipe','four_finger_swipe')]
    [string]$Gesture = 'all'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$desktop = Join-Path $root 'target\debug\capyio-desktop.exe'
$expectedHash = 'BC052B4616CBED7121513577439847C3A6D1BC904E1DE7676716A0B2FEAC3CF5'
$evaluator = Join-Path $PSScriptRoot 'android_webview_cdp_eval.mjs'
$matrix = Join-Path $PSScriptRoot 'android_touchpad_cdp_gesture_matrix.mjs'
$discovery = 'http://127.0.0.1:9223/json/list'
$evidence = Join-Path $root 'target\lab-evidence'
$gateOutput = Join-Path $evidence 'CAPY-INTEGRATED-touchpad-gestures-gate.stdout.txt'
$gateError = Join-Path $evidence 'CAPY-INTEGRATED-touchpad-gestures-gate.stderr.txt'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The paired gesture matrix requires elevated PowerShell for the existing VHF interface.'
}
if ((Get-FileHash -LiteralPath $desktop -Algorithm SHA256).Hash -ne $expectedHash) {
    throw 'Embedded CapyIO Desktop hash mismatch.'
}
New-Item -ItemType Directory -Path $evidence -Force | Out-Null
Start-Transcript -LiteralPath (Join-Path $evidence 'CAPY-INTEGRATED-touchpad-gestures.txt') -Force
try {
    $gate = Start-Process -FilePath $desktop -ArgumentList @(
        '--verify-paired-touchpad-data', $NodeId, $HostAddress, $ControlPort,
        '--acknowledge-desktop-input'
    ) -WindowStyle Hidden -RedirectStandardOutput $gateOutput `
      -RedirectStandardError $gateError -PassThru
    $route = $null
    $active = $false
    for ($attempt = 0; $attempt -lt 200; $attempt += 1) {
        if (-not $route -and (Test-Path -LiteralPath $gateOutput)) {
            $output = Get-Content -LiteralPath $gateOutput -Raw
            if ($output -match 'CAPYIO_RESOURCE_CONSENT_PENDING route=([0-9a-fA-F-]{36})') {
                $route = $Matches[1]
                $approved = & node $evaluator $discovery `
                    "window.CapyIOAndroid.resolveResourceRequest('$route',true)" 2>$null
                if ($approved -notmatch 'true') { throw "Exact Route approval failed: $route" }
                Write-Output "approved_route=$route"
            }
        }
        $state = & node $evaluator $discovery `
            "JSON.stringify({touchpad:JSON.parse(window.CapyIOAndroid.getTouchpadStateJson()),surface:document.querySelector('.touchpad-surface--immersive')!==null})" 2>$null
        if ($state -match '\\"status\\":\\"active\\"' -and $state -match '\\"surface\\":true') {
            $active = $true
            break
        }
        Start-Sleep -Milliseconds 150
    }
    if (-not $active -or -not $route) { throw 'Authenticated touchpad surface did not become active.' }
    Write-Output "android_active=$state"
    $results = @(& node $matrix $discovery $Gesture 2>&1)
    $matrixExit = $LASTEXITCODE
    $results | Write-Output
    if ($matrixExit -ne 0) { throw "Gesture injection failed: $matrixExit" }
    $completed = @($results | Where-Object { $_ -match '^\{"gesture":' }).Count
    $expected = if ($Gesture -eq 'all') { 6 } else { 1 }
    if ($completed -ne $expected) { throw "Expected $expected Android gesture traces; observed $completed" }
    if (-not $gate.WaitForExit(30000)) { throw 'Paired touchpad gate did not stop in time.' }
    $gate.WaitForExit()
    $output = Get-Content -LiteralPath $gateOutput -Raw
    if ($output -notmatch "CAPYIO_TOUCHPAD_DATA_PASSED route=$([regex]::Escape($route)) transport=mtls") {
        throw 'Authenticated data gate did not confirm this Route.'
    }
    Write-Output 'CAPYIO_TOUCHPAD_GESTURE_MATRIX_TRANSPORT_PASSED=true'
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
