[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Driver rollback requires an elevated administrator PowerShell.'
}

$hardwareId = 'ROOT\CAPYIOVHFTOUCHPAD'
$devices = @(
    Get-PnpDevice -ErrorAction SilentlyContinue |
        Where-Object {
            $ids = @(
                Get-PnpDeviceProperty -InstanceId $_.InstanceId `
                    -KeyName 'DEVPKEY_Device_HardwareIds' `
                    -ErrorAction SilentlyContinue
            ).Data
            @($ids | Where-Object { $null -ne $_ } | ForEach-Object { $_.ToString().ToUpperInvariant() }) -contains $hardwareId
        }
)
foreach ($device in $devices) {
    & pnputil.exe /remove-device $device.InstanceId
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to remove exact device $($device.InstanceId): pnputil $LASTEXITCODE"
    }
}

$packages = @(
    Get-WindowsDriver -Online -All |
        Where-Object {
            $_.ProviderName -eq 'CapyIO' -and
            (Split-Path -Leaf $_.OriginalFileName) -ieq 'CapyIOVhfTouchpad.inf'
        }
)
if ($packages.Count -gt 1) {
    throw "Refusing ambiguous rollback: found $($packages.Count) CapyIO VHF packages"
}
if ($packages.Count -eq 1) {
    $publishedName = $packages[0].Driver
    if ($publishedName -notmatch '^oem[0-9]+\.inf$') {
        throw "Refusing unexpected published driver name: $publishedName"
    }
    & pnputil.exe /delete-driver $publishedName /uninstall /force
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to delete exact package ${publishedName}: pnputil $LASTEXITCODE"
    }
}

$remaining = @(
    Get-PnpDevice -ErrorAction SilentlyContinue |
        Where-Object {
            $ids = @(
                Get-PnpDeviceProperty -InstanceId $_.InstanceId `
                    -KeyName 'DEVPKEY_Device_HardwareIds' `
                    -ErrorAction SilentlyContinue
            ).Data
            @($ids | Where-Object { $null -ne $_ } | ForEach-Object { $_.ToString().ToUpperInvariant() }) -contains $hardwareId
        }
)
if ($remaining.Count -ne 0) {
    throw "Rollback left $($remaining.Count) matching device instances"
}

$testThumbprint = '6D771D01DFED85EE5F4473F7449C093182F0960D'
$certificatesRemoved = 0
foreach ($store in @('Cert:\LocalMachine\TrustedPublisher', 'Cert:\LocalMachine\Root')) {
    $certificatePath = Join-Path $store $testThumbprint
    if (Test-Path -LiteralPath $certificatePath) {
        Remove-Item -LiteralPath $certificatePath -Force
        $certificatesRemoved += 1
    }
}

Write-Output "CapyIO VHF rollback complete; devices_removed=$($devices.Count); packages_removed=$($packages.Count); certificates_removed=$certificatesRemoved"
