[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$expectedInstanceId = 'ROOT\SYSTEM\0001'
$expectedHardwareId = 'ROOT\CAPYIOVHFTOUCHPAD'
$expectedPublishedInf = 'oem175.inf'
$expectedThumbprint = '6D771D01DFED85EE5F4473F7449C093182F0960D'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003F-rollback.txt'
New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null

Start-Transcript -LiteralPath $evidencePath -Force
try {
    $device = Get-PnpDevice -InstanceId $expectedInstanceId -ErrorAction SilentlyContinue
    if ($null -ne $device) {
        $hardwareIds = @(Get-PnpDeviceProperty -InstanceId $expectedInstanceId `
            -KeyName 'DEVPKEY_Device_HardwareIds').Data
        $canonicalIds = @($hardwareIds | Where-Object { $null -ne $_ } |
            ForEach-Object { $_.ToString().ToUpperInvariant() })
        if ($canonicalIds -notcontains $expectedHardwareId) {
            throw "Refusing instance mismatch: $expectedInstanceId has [$($canonicalIds -join ', ')]"
        }
        & pnputil.exe /remove-device $expectedInstanceId
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to remove ${expectedInstanceId}: pnputil $LASTEXITCODE"
        }
    }

    $packages = @(Get-WindowsDriver -Online -All | Where-Object Driver -EQ $expectedPublishedInf)
    if ($packages.Count -gt 1) {
        throw "Expected at most one $expectedPublishedInf package, found $($packages.Count)"
    }
    if ($packages.Count -eq 1) {
        $package = $packages[0]
        if ($package.ProviderName -ne 'CapyIO' -or
            (Split-Path -Leaf $package.OriginalFileName) -ine 'CapyIOVhfTouchpad.inf') {
            throw "Refusing package mismatch for $expectedPublishedInf"
        }
        & pnputil.exe /delete-driver $expectedPublishedInf /uninstall /force
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to delete ${expectedPublishedInf}: pnputil $LASTEXITCODE"
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
    Write-Output "003F rollback complete; certificates_removed=$certificatesRemoved"
}
catch {
    Write-Output "003F rollback failed: $($_.Exception.Message)"
    throw
}
finally {
    Stop-Transcript
}
