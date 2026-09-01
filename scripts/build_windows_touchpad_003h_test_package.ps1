[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$driverRoot = Join-Path $repositoryRoot 'drivers\windows-touchpad'
$sourceInf = Join-Path $driverRoot 'CapyIOVhfTouchpad.inf'
$sourceSys = Join-Path $driverRoot 'x64\Debug\CapyIOVhfTouchpad.sys'
$packageRoot = Join-Path $repositoryRoot 'target\lab-packages\CapyIOVhfTouchpad-0.0.2.0-x64'
$inf2Cat = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x86\Inf2Cat.exe'
$signTool = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe'

$expectedInfHash = 'F6886A1C6535B91D4D886B17C2B2A7245006BCBCBA9F359C9731AAC45CA9E02F'
$expectedSysHash = '148D02454740C3413AE741C14A35F86D6096DFE4F83DDD76235A1669EB97862E'
$subject = 'CN=CapyIO Local Lab Driver Test 003H'
$certificate = $null

function Assert-ExactHash {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Expected
    )
    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
    if ($actual -ne $Expected) {
        throw "Input hash mismatch for ${Path}; expected ${Expected}, received ${actual}"
    }
}

foreach ($required in @($sourceInf, $sourceSys, $inf2Cat, $signTool)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Required file is absent: $required"
    }
}
if (Test-Path -LiteralPath $packageRoot) {
    throw "Refusing to overwrite existing package directory: $packageRoot"
}

Assert-ExactHash -Path $sourceInf -Expected $expectedInfHash
Assert-ExactHash -Path $sourceSys -Expected $expectedSysHash

try {
    New-Item -ItemType Directory -Path $packageRoot | Out-Null
    $packageInf = Join-Path $packageRoot 'CapyIOVhfTouchpad.inf'
    $packageSys = Join-Path $packageRoot 'CapyIOVhfTouchpad.sys'
    $packageCat = Join-Path $packageRoot 'CapyIOVhfTouchpad.cat'
    $packageCer = Join-Path $packageRoot 'CapyIOVhfTouchpad-Test.cer'
    Copy-Item -LiteralPath $sourceInf -Destination $packageInf
    Copy-Item -LiteralPath $sourceSys -Destination $packageSys

    $certificate = New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject $subject `
        -CertStoreLocation 'Cert:\CurrentUser\My' `
        -KeyAlgorithm RSA `
        -KeyLength 3072 `
        -HashAlgorithm SHA256 `
        -KeyExportPolicy NonExportable `
        -NotAfter (Get-Date).AddDays(30)

    & $signTool sign /fd SHA256 /sha1 $certificate.Thumbprint /s My $packageSys
    if ($LASTEXITCODE -ne 0) {
        throw "signtool failed to sign SYS with exit code $LASTEXITCODE"
    }
    & $inf2Cat "/driver:$packageRoot" /os:10_X64
    if ($LASTEXITCODE -ne 0) {
        throw "Inf2Cat failed with exit code $LASTEXITCODE"
    }
    & $signTool sign /fd SHA256 /sha1 $certificate.Thumbprint /s My $packageCat
    if ($LASTEXITCODE -ne 0) {
        throw "signtool failed to sign CAT with exit code $LASTEXITCODE"
    }
    Export-Certificate -Cert $certificate -FilePath $packageCer -Type CERT | Out-Null

    Get-ChildItem -LiteralPath $packageRoot -File | Sort-Object Name | ForEach-Object {
        $signature = Get-AuthenticodeSignature -LiteralPath $_.FullName
        [pscustomobject]@{
            Name = $_.Name
            Length = $_.Length
            SHA256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
            SignatureStatus = $signature.Status.ToString()
            Signer = $signature.SignerCertificate.Subject
            SignerThumbprint = $signature.SignerCertificate.Thumbprint
        }
    } | ConvertTo-Json -Depth 3
}
finally {
    if ($null -ne $certificate) {
        Remove-Item -LiteralPath "Cert:\CurrentUser\My\$($certificate.Thumbprint)" -Force
    }
}
