[CmdletBinding()]
param()

$ErrorActionPreference = 'Continue'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003G-driver-state.txt'
$devcon = 'C:\Program Files (x86)\Windows Kits\10\Tools\10.0.26100.0\x64\devcon.exe'
$probe = Join-Path $repositoryRoot 'target\debug\capyio-ptp-probe.exe'
New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null

Start-Transcript -LiteralPath $evidencePath -Force
try {
    Write-Output '--- Exact root device ---'
    $devices = @(Get-PnpDevice | Where-Object {
        $ids = @(Get-PnpDeviceProperty -InstanceId $_.InstanceId `
            -KeyName 'DEVPKEY_Device_HardwareIds' -ErrorAction SilentlyContinue).Data
        @($ids | Where-Object { $null -ne $_ } | ForEach-Object { $_.ToString().ToUpperInvariant() }) -contains 'ROOT\CAPYIOVHFTOUCHPAD'
    })
    $devices | Select-Object Status, Class, FriendlyName, InstanceId, Problem,
        ConfigManagerErrorCode | Format-List
    foreach ($device in $devices) {
        Get-PnpDeviceProperty -InstanceId $device.InstanceId |
            Where-Object KeyName -Match 'Problem|ProblemStatus|HardwareIds|Service' |
            Select-Object KeyName, Type, Data | Format-List
    }
    Write-Output '--- DevCon status/stack ---'
    & $devcon status 'Root\CapyIOVhfTouchpad'
    & $devcon stack 'Root\CapyIOVhfTouchpad'
    Write-Output '--- Service ---'
    sc.exe query CapyIOVhfTouchpad
    sc.exe qc CapyIOVhfTouchpad
    Write-Output '--- Elevated interface probe ---'
    if (Test-Path -LiteralPath $probe) {
        & $probe --vhf-interface
    }
    Write-Output '--- Recent System events ---'
    Get-WinEvent -FilterHashtable @{ LogName = 'System'; StartTime = (Get-Date).AddMinutes(-20) } |
        Where-Object {
            $_.ProviderName -match 'Kernel-PnP|Service Control Manager|DriverFrameworks' -or
            $_.Message -match 'CapyIO|CapyIOVhfTouchpad'
        } |
        Select-Object -First 60 TimeCreated, Id, LevelDisplayName, ProviderName, Message |
        Format-List
}
finally {
    Stop-Transcript
}
