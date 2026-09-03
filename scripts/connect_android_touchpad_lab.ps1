[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[A-Za-z0-9._:-]+$')]
    [string]$AdbSerial,

    [switch]$StartReceiver,

    [ValidatePattern('^[0-9]+\.[0-9]+$')]
    [string]$ExpectedAppVersion = '1.10'
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$adb = Join-Path $env:LOCALAPPDATA 'Android\Sdk\platform-tools\adb.exe'
$receiverScript = Join-Path $PSScriptRoot 'run_windows_touchpad_003z_android_vhf_persistent.ps1'
$package = 'dev.capyio.touchpad.lab'
$activity = 'dev.capyio.touchpad.lab/.TouchpadLabActivity'
$port = 61000

function Invoke-CapyAdb {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)

    $output = @(& $adb -s $AdbSerial @Arguments 2>&1)
    if ($LASTEXITCODE -ne 0) {
        throw "ADB command failed ($LASTEXITCODE): $($output -join [Environment]::NewLine)"
    }
    return $output
}

function Test-CapyListener {
    $properties = [Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties()
    $listener = $properties.GetActiveTcpListeners() |
        Where-Object { $_.Address.ToString() -eq '127.0.0.1' -and $_.Port -eq $port } |
        Select-Object -First 1
    return $null -ne $listener
}

function Test-CapyEstablished {
    $properties = [Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties()
    $connection = $properties.GetActiveTcpConnections() |
        Where-Object {
            $_.LocalEndPoint.Address.ToString() -eq '127.0.0.1' -and
            $_.LocalEndPoint.Port -eq $port -and
            $_.State -eq [Net.NetworkInformation.TcpState]::Established
        } |
        Select-Object -First 1
    return $null -ne $connection
}

if (-not (Test-Path -LiteralPath $adb -PathType Leaf)) {
    throw "Android SDK adb is absent: $adb"
}
if (-not (Test-Path -LiteralPath $receiverScript -PathType Leaf)) {
    throw "Pinned VHF receiver wrapper is absent: $receiverScript"
}

$state = (Invoke-CapyAdb -Arguments @('get-state') | Select-Object -Last 1).Trim()
if ($state -ne 'device') {
    throw "ADB endpoint is not ready: serial=$AdbSerial state=$state"
}

$packageDump = Invoke-CapyAdb -Arguments @('shell', 'dumpsys', 'package', $package)
$versionLine = $packageDump | Select-String '^\s*versionName=' | Select-Object -First 1
if ($null -eq $versionLine) {
    throw "CapyIO touchpad lab package is not installed: $package"
}
$actualVersion = ($versionLine.Line -split '=', 2)[1].Trim()
if ($actualVersion -ne $ExpectedAppVersion) {
    throw "CapyIO touchpad lab version mismatch: expected=$ExpectedAppVersion actual=$actualVersion"
}

if (-not (Test-CapyListener)) {
    if (-not $StartReceiver) {
        throw "Windows receiver is not listening on 127.0.0.1:$port. Re-run with -StartReceiver to request the separately gated elevated VHF session."
    }
    Start-Process `
        -FilePath 'powershell.exe' `
        -Verb RunAs `
        -WindowStyle Hidden `
        -ArgumentList @(
            '-NoProfile',
            '-ExecutionPolicy',
            'Bypass',
            '-File',
            ('"' + $receiverScript + '"')
        ) | Out-Null
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    while (-not (Test-CapyListener) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 250
    }
    if (-not (Test-CapyListener)) {
        throw 'The elevated VHF receiver did not begin listening within 20 seconds.'
    }
}

$reverseResult = Invoke-CapyAdb -Arguments @(
    'reverse',
    "tcp:$port",
    "tcp:$port"
)

$reusedSession = Test-CapyEstablished
if (-not $reusedSession) {
    Invoke-CapyAdb -Arguments @('shell', 'am', 'force-stop', $package) | Out-Null
    Invoke-CapyAdb -Arguments @('shell', 'am', 'start', '-n', $activity) | Out-Null
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-CapyEstablished) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 250
    }
    if (-not (Test-CapyEstablished)) {
        throw 'Android Activity did not establish the loopback touchpad session within 10 seconds.'
    }
}

$activityDump = Invoke-CapyAdb -Arguments @('shell', 'dumpsys', 'activity', 'activities')
$topResumed = $activityDump | Select-String 'topResumedActivity=.*dev\.capyio\.touchpad\.lab/\.TouchpadLabActivity' |
    Select-Object -First 1
if ($null -eq $topResumed) {
    throw 'CapyIO touchpad Activity is connected but is not the top-resumed Android Activity.'
}

Write-Output 'touchpad_lab_connection=ready'
Write-Output "adb_serial=$AdbSerial"
Write-Output "android_app_version=$actualVersion"
Write-Output "adb_reverse=$($reverseResult | Select-Object -Last 1)"
Write-Output "receiver_listener=127.0.0.1:$port"
Write-Output "session_reused=$($reusedSession.ToString().ToLowerInvariant())"
Write-Output 'android_top_resumed=true'
Write-Output 'driver_or_apk_installation_performed=false'
Write-Output 'windows_restart_performed=false'
