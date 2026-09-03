[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$packageRoot = Join-Path $repositoryRoot 'target\lab-packages\CapyIOVhfTouchpad-0.0.1.0-x64'
$evidenceRoot = Join-Path $repositoryRoot 'target\lab-evidence'
$evidencePath = Join-Path $evidenceRoot 'CAPY-PTP-003G-install.txt'
$rollback = Join-Path $PSScriptRoot 'remove_windows_touchpad_test_driver.ps1'
$devcon = 'C:\Program Files (x86)\Windows Kits\10\Tools\10.0.26100.0\x64\devcon.exe'
$signTool = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe'
$hardwareId = 'Root\CapyIOVhfTouchpad'
$expectedThumbprint = '6D771D01DFED85EE5F4473F7449C093182F0960D'
$expectedHashes = @{
    'CapyIOVhfTouchpad-Test.cer' = '81F0C6B791AB20CA74B97536206378DE70D6B6918C73E84B721141E009B4A79E'
    'capyiovhftouchpad.cat' = 'CED8DFCE05C30E6439937863AE026C8274E4A585E0FCFD06B4F7710592F53DE8'
    'CapyIOVhfTouchpad.inf' = 'CC034E0FE8DEA161B47DD8C6E84218419F3F62CB70DD8B5DC18BEC9E3EE47514'
    'CapyIOVhfTouchpad.sys' = '1F1F0BC738DAE54D5B3FE54983524C66554B33AAFEAAC4832E084F2BA8B951BC'
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
    if ($LASTEXITCODE -ne 0) {
        throw "SYS signature verification failed: $LASTEXITCODE"
    }
    & $signTool verify /pa /v (Join-Path $packageRoot 'capyiovhftouchpad.cat')
    if ($LASTEXITCODE -ne 0) {
        throw "CAT signature verification failed: $LASTEXITCODE"
    }

    & $devcon install (Join-Path $packageRoot 'CapyIOVhfTouchpad.inf') $hardwareId
    $devconExit = $LASTEXITCODE
    if ($devconExit -eq 1) {
        throw 'DevCon requested a restart; refusing to retain a restart-pending installation.'
    }
    if ($devconExit -ne 0) {
        throw "DevCon installation failed: $devconExit"
    }

    Get-PnpDevice | Where-Object InstanceId -Like 'ROOT\CAPYIOVHFTOUCHPAD\*' |
        Select-Object Status, Class, FriendlyName, InstanceId | Format-List
    Write-Output 'CAPY-PTP-003G installation completed without requesting a restart.'
}
catch {
    Write-Error $_
    & $rollback
    throw
}
finally {
    Stop-Transcript
}
