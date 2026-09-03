[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$expectedHardwareId = 'ROOT\CAPYIOVHFTOUCHPAD'
$expectedThumbprint = 'D6EF151680FAC70FDF34623DEC23041D76372D7E'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003H-rollback.txt'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Driver rollback requires an elevated administrator PowerShell.'
}

New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Start-Transcript -LiteralPath $evidencePath -Force
try {
    $devices = @(
        Get-PnpDevice -Class System -ErrorAction SilentlyContinue |
            Where-Object {
                $property = Get-PnpDeviceProperty -InstanceId $_.InstanceId `
                    -KeyName 'DEVPKEY_Device_HardwareIds' -ErrorAction SilentlyContinue
                $ids = if ($null -eq $property) { @() } else { @($property.Data) }
                @($ids | Where-Object { $null -ne $_ } |
                    ForEach-Object { $_.ToString().ToUpperInvariant() }) -contains $expectedHardwareId
            }
    )
    if ($devices.Count -gt 1) {
        throw "Refusing ambiguous rollback: found $($devices.Count) matching devices"
    }
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
                (Split-Path -Leaf $_.OriginalFileName) -ieq 'CapyIOVhfTouchpad.inf' -and
                $_.Version -eq [version]'0.0.2.0'
            }
    )
    if ($packages.Count -gt 1) {
        throw "Refusing ambiguous rollback: found $($packages.Count) matching packages"
    }
    foreach ($package in $packages) {
        if ($package.Driver -notmatch '^oem[0-9]+\.inf$') {
            throw "Refusing unexpected published driver name: $($package.Driver)"
        }
        & pnputil.exe /delete-driver $package.Driver /uninstall /force
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to delete exact package $($package.Driver): pnputil $LASTEXITCODE"
        }
    }

    $servicePath = 'HKLM:\SYSTEM\CurrentControlSet\Services\CapyIOVhfTouchpad'
    if (Test-Path -LiteralPath $servicePath) {
        $imagePath = (Get-ItemProperty -LiteralPath $servicePath -Name ImagePath).ImagePath
        if ($imagePath -notmatch '(?i)\\capyiovhftouchpad\.inf_amd64_[^\\]+\\CapyIOVhfTouchpad\.sys$') {
            throw "Refusing unexpected service ImagePath: $imagePath"
        }
        & sc.exe delete CapyIOVhfTouchpad
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to delete exact service: sc.exe $LASTEXITCODE"
        }
    }

    $certificatesRemoved = 0
    foreach ($store in @('Cert:\LocalMachine\TrustedPublisher', 'Cert:\LocalMachine\Root')) {
        $certificatePath = Join-Path $store $expectedThumbprint
        if (Test-Path -LiteralPath $certificatePath) {
            Remove-Item -LiteralPath $certificatePath -Force
            $certificatesRemoved += 1
        }
    }
    Write-Output "003H rollback complete; devices_removed=$($devices.Count); packages_removed=$($packages.Count); certificates_removed=$certificatesRemoved"
}
finally {
    Stop-Transcript
}
