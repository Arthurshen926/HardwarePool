[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$packageRoot = Join-Path $repositoryRoot 'target\lab-packages\CapyIOVhfTouchpad-0.0.2.0-x64'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003H-install.txt'
$rollback = Join-Path $PSScriptRoot 'remove_windows_touchpad_003h_test_driver.ps1'
$devcon = 'C:\Program Files (x86)\Windows Kits\10\Tools\10.0.26100.0\x64\devcon.exe'
$signTool = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe'
$hardwareId = 'Root\CapyIOVhfTouchpad'
$expectedThumbprint = 'D6EF151680FAC70FDF34623DEC23041D76372D7E'
$expectedHashes = @{
    'CapyIOVhfTouchpad-Test.cer' = '2FC60146855B8831FD567E9821B741BC66931AE761B25BD41BC293048B6F9DE7'
    'capyiovhftouchpad.cat' = '8A539E9619ED9C4036BFAD11778CD8B78B77D54567415AA1C080CDF34DB13C7D'
    'CapyIOVhfTouchpad.inf' = 'F6886A1C6535B91D4D886B17C2B2A7245006BCBCBA9F359C9731AAC45CA9E02F'
    'CapyIOVhfTouchpad.sys' = 'A1380349DCC42FDB654D5A9B3A212A72D61A105443FDF1AE7AE7917F39096F10'
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Driver installation requires an elevated administrator PowerShell.'
}
foreach ($required in @($packageRoot, $rollback, $devcon, $signTool)) {
    if (-not (Test-Path -LiteralPath $required)) {
        throw "Required deployment input is absent: $required"
    }
}
foreach ($entry in $expectedHashes.GetEnumerator()) {
    $path = Join-Path $packageRoot $entry.Key
    $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
    if ($actual -ne $entry.Value) {
        throw "Package hash mismatch for $($entry.Key): $actual"
    }
}

$bootEntry = bcdedit.exe /enum '{current}' | Out-String
if ($bootEntry -notmatch '(?im)^testsigning\s+Yes\s*$') {
    throw 'Current boot entry is not already running with testsigning Yes; refusing to modify BCD or request a reboot.'
}
if (Confirm-SecureBootUEFI) {
    throw 'Secure Boot is enabled; refusing this local self-signed package.'
}

New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Start-Transcript -LiteralPath $evidencePath -Force
try {
    $certificatePath = Join-Path $packageRoot 'CapyIOVhfTouchpad-Test.cer'
    $certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new($certificatePath)
    if ($certificate.Thumbprint -ne $expectedThumbprint) {
        throw "Certificate thumbprint mismatch: $($certificate.Thumbprint)"
    }
    Import-Certificate -FilePath $certificatePath -CertStoreLocation 'Cert:\LocalMachine\Root' | Out-Null
    Import-Certificate -FilePath $certificatePath -CertStoreLocation 'Cert:\LocalMachine\TrustedPublisher' | Out-Null

    & $signTool verify /pa /v (Join-Path $packageRoot 'CapyIOVhfTouchpad.sys')
    if ($LASTEXITCODE -ne 0) { throw "SYS signature verification failed: $LASTEXITCODE" }
    & $signTool verify /pa /v (Join-Path $packageRoot 'capyiovhftouchpad.cat')
    if ($LASTEXITCODE -ne 0) { throw "CAT signature verification failed: $LASTEXITCODE" }

    & $devcon install (Join-Path $packageRoot 'CapyIOVhfTouchpad.inf') $hardwareId
    $devconExit = $LASTEXITCODE
    if ($devconExit -eq 1) {
        throw 'DevCon requested a restart; refusing to retain a restart-pending installation.'
    }
    if ($devconExit -ne 0) { throw "DevCon installation failed: $devconExit" }

    $device = Get-PnpDevice -Class System -ErrorAction SilentlyContinue |
        Where-Object FriendlyName -eq 'CapyIO VHF Precision Touchpad Source'
    if (@($device).Count -ne 1 -or $device.Status -ne 'OK') {
        throw 'The exact CapyIO source device did not reach PnP status OK.'
    }
    Write-Output 'CAPY-PTP-003H installation completed and device status is OK; no restart requested.'
}
catch {
    Write-Error $_
    & $rollback
    throw
}
finally {
    Stop-Transcript
}
