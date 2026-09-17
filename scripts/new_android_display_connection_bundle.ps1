param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('cpd1', 'gamestream')]
    [string]$Transport,
    [Parameter(Mandatory = $true, ParameterSetName = 'File')]
    [string]$ConnectionProperties,
    [Parameter(Mandatory = $true, ParameterSetName = 'Content')]
    [string]$ConnectionPropertiesText,
    [Parameter(Mandatory = $true)] [string]$CaCertificate,
    [Parameter(Mandatory = $true)] [string]$ClientCertificate,
    [Parameter(Mandatory = $true)] [string]$ClientPrivateKey,
    [Parameter(Mandatory = $true)] [string]$Output
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression

function Resolve-BoundedInput([string]$Path, [long]$MaximumBytes, [string]$Label) {
    $resolved = (Resolve-Path -LiteralPath $Path -ErrorAction Stop).Path
    $item = Get-Item -LiteralPath $resolved -Force
    if (-not $item.PSIsContainer -and $item.Length -gt 0 -and $item.Length -le $MaximumBytes) {
        return $item
    }
    throw "$Label must be a non-empty regular file no larger than $MaximumBytes bytes."
}

$property = $null
$propertyBytes = $null
if ($PSCmdlet.ParameterSetName -eq 'File') {
    $property = Resolve-BoundedInput $ConnectionProperties 8192 'Connection properties'
} else {
    $propertyBytes = [System.Text.Encoding]::GetEncoding(28591).GetBytes($ConnectionPropertiesText)
    if ($propertyBytes.Length -eq 0 -or $propertyBytes.Length -gt 8192) {
        throw 'Connection properties text must encode to 1..8192 ISO-8859-1 bytes.'
    }
}
$ca = Resolve-BoundedInput $CaCertificate 65536 'CA certificate'
$certificate = Resolve-BoundedInput $ClientCertificate 65536 'Client certificate'
$key = Resolve-BoundedInput $ClientPrivateKey 65536 'Client private key'
$outputPath = [System.IO.Path]::GetFullPath($Output)
$outputParent = [System.IO.Path]::GetDirectoryName($outputPath)
if (-not [System.IO.Directory]::Exists($outputParent)) {
    throw 'The output directory does not exist.'
}
if ([System.IO.File]::Exists($outputPath) -or [System.IO.Directory]::Exists($outputPath)) {
    throw 'The output path already exists; refusing to overwrite a credential bundle.'
}

$propertiesEntryName = if ($Transport -eq 'cpd1') {
    'connection.properties'
} else {
    'gamestream-launch.properties'
}
$inputs = [ordered]@{
    'transport.properties' = $null
    $propertiesEntryName = if ($null -ne $property) { $property } else { $propertyBytes }
    'ca.der' = $ca
    'client-cert.der' = $certificate
    'client-key.pk8' = $key
}

$stream = $null
$archive = $null
try {
    $stream = [System.IO.File]::Open(
        $outputPath,
        [System.IO.FileMode]::CreateNew,
        [System.IO.FileAccess]::ReadWrite,
        [System.IO.FileShare]::None)
    $archive = [System.IO.Compression.ZipArchive]::new(
        $stream,
        [System.IO.Compression.ZipArchiveMode]::Create,
        $true)
    foreach ($entryName in $inputs.Keys) {
        $entry = $archive.CreateEntry($entryName, [System.IO.Compression.CompressionLevel]::Optimal)
        $entry.LastWriteTime = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
        $destination = $entry.Open()
        try {
            if ($entryName -eq 'transport.properties') {
                $bytes = [System.Text.Encoding]::ASCII.GetBytes("mode=$Transport`n")
                $destination.Write($bytes, 0, $bytes.Length)
            } elseif ($entryName -eq $propertiesEntryName -and $null -ne $propertyBytes) {
                $destination.Write($propertyBytes, 0, $propertyBytes.Length)
            } else {
                $source = [System.IO.File]::OpenRead($inputs[$entryName].FullName)
                try { $source.CopyTo($destination) } finally { $source.Dispose() }
            }
        } finally {
            $destination.Dispose()
        }
    }
    $archive.Dispose()
    $archive = $null
    $stream.Flush($true)
    $stream.Dispose()
    $stream = $null
} catch {
    if ($archive -ne $null) { $archive.Dispose() }
    if ($stream -ne $null) { $stream.Dispose() }
    if ([System.IO.File]::Exists($outputPath)) {
        [System.IO.File]::Delete($outputPath)
    }
    throw
} finally {
    if ($null -ne $propertyBytes) {
        [Array]::Clear($propertyBytes, 0, $propertyBytes.Length)
    }
}

$artifact = Get-Item -LiteralPath $outputPath
$digest = (Get-FileHash -LiteralPath $outputPath -Algorithm SHA256).Hash
[pscustomobject]@{
    path = $artifact.FullName
    transport = $Transport
    bytes = $artifact.Length
    sha256 = $digest
}
