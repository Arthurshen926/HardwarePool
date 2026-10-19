[CmdletBinding()]
param(
    [string]$PackageRoot = $PSScriptRoot,
    [switch]$CheckPrerequisites
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# This is an integrity check, not a signature or a trust decision. It never
# launches a packaged executable, connects to a device, or modifies the host.
$expectedFiles = @(
    'capyio-desktop.exe', 'capyio-ptp-adb-lab.exe',
    'capyio-avc-lab-receiver.exe', 'capyio-camera-session-host.exe',
    'android/CapyIO-debug.apk', 'README.md', 'LICENSE',
    'verify_integrated_test_package.ps1'
)
$root = Get-Item -LiteralPath $PackageRoot
if (-not $root.PSIsContainer -or ($root.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw 'Package root must be an ordinary directory, not a link.'
}
$manifestFile = Get-Item -LiteralPath (Join-Path $root.FullName 'manifest.json')
if ($manifestFile.PSIsContainer -or $manifestFile.Length -gt 65536 -or
        ($manifestFile.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw 'Invalid package manifest file.'
}
$manifest = Get-Content -LiteralPath $manifestFile.FullName -Raw | ConvertFrom-Json
if ($manifest.schemaVersion -ne 2 -or $manifest.kind -cne 'capyio-local-test-delivery' -or
        $manifest.productionRelease -ne $false -or @($manifest.artifacts).Count -ne $expectedFiles.Count) {
    throw 'Unsupported or incomplete local-test package manifest.'
}
$seen = @{}
$totalBytes = [long]0
foreach ($entry in $manifest.artifacts) {
    $name = [string]$entry.file
    if ($expectedFiles -cnotcontains $name -or $seen.ContainsKey($name)) {
        throw "Unexpected or duplicate package entry: $name"
    }
    $seen[$name] = $true
    if ([string]$entry.sha256 -cnotmatch '^[0-9A-F]{64}$' -or
            $entry.bytes -isnot [ValueType] -or $entry.bytes -le 0 -or $entry.bytes -gt 536870912) {
        throw "Invalid package digest or size: $name"
    }
    $relativeParts = $name.Split('/')
    $path = $root.FullName
    foreach ($part in $relativeParts) {
        $path = Join-Path $path $part
        $item = Get-Item -LiteralPath $path
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw "Package entries cannot be links: $name"
        }
    }
    if ($item.PSIsContainer -or $item.Length -ne $entry.bytes) {
        throw "Missing or changed package file size: $name"
    }
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -cne $entry.sha256) {
        throw "Package checksum mismatch: $name"
    }
    $totalBytes += $item.Length
}

# Refuse extra files (especially accidental invitations/keys) and linked
# directories before descending, rather than recursively following a junction.
$pendingDirectories = [Collections.Generic.Queue[string]]::new()
$pendingDirectories.Enqueue($root.FullName)
$observedFiles = 0
while ($pendingDirectories.Count -gt 0) {
    foreach ($item in Get-ChildItem -LiteralPath $pendingDirectories.Dequeue() -Force) {
        $relative = $item.FullName.Substring($root.FullName.TrimEnd('\', '/').Length + 1).Replace('\', '/')
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw "Package entries cannot be links: $relative"
        }
        if ($item.PSIsContainer) {
            if ($relative -cne 'android') { throw "Unexpected package directory: $relative" }
            $pendingDirectories.Enqueue($item.FullName)
        } else {
            if ($relative -cne 'manifest.json' -and $expectedFiles -cnotcontains $relative) {
                throw "Unexpected package file: $relative"
            }
            $observedFiles++
        }
    }
}
if ($observedFiles -ne $expectedFiles.Count + 1) { throw 'Package inventory is incomplete.' }

Write-Output "Package integrity: PASS ($($expectedFiles.Count) files, $totalBytes bytes)"
Write-Output "Android APK signer SHA-256: $($manifest.android.signerSha256)"
Write-Output "Build provenance: $($manifest.buildProvenance)"
Write-Output 'Local test build only. Checksums detect changed bytes; they do not prove publisher identity or hardware acceptance.'

if ($CheckPrerequisites) {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or -not [Environment]::Is64BitOperatingSystem) {
        throw 'The packaged Windows application requires 64-bit Windows.'
    }
    $webViewVersions = @()
    foreach ($base in @(
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients',
        'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients',
        'HKCU:\Software\Microsoft\EdgeUpdate\Clients'
    )) {
        if (Test-Path -LiteralPath $base) {
            foreach ($client in Get-ChildItem -LiteralPath $base) {
                $record = Get-ItemProperty -LiteralPath $client.PSPath
                if ($record.PSObject.Properties.Name -contains 'name' -and
                        $record.name -match 'WebView2' -and
                        $record.PSObject.Properties.Name -contains 'pv' -and $record.pv -ne '0.0.0.0') {
                    $webViewVersions += $record.pv
                }
            }
        }
    }
    if ($webViewVersions.Count -eq 0) {
        Write-Warning 'WebView2 Runtime was not found in its standard registry locations. Install/repair it through Microsoft before launching if the UI fails.'
    } else {
        Write-Output "WebView2 Runtime: $($webViewVersions -join ', ')"
    }
    $broker = Get-Service -Name CapyIOBroker -ErrorAction SilentlyContinue
    if ($null -eq $broker) {
        Write-Output 'System audio/display broker: not installed. These projections need separate deployment; node connection does not.'
    } else {
        Write-Output "System audio/display broker service: $($broker.Status) (not an end-to-end readiness result)"
    }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try {
        $principal = [Security.Principal.WindowsPrincipal]::new($identity)
        Write-Output "Current Windows pairing account SID: $($identity.User.Value)"
        Write-Output "Administrator token: $($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator))"
    } finally { $identity.Dispose() }
    Write-Output 'Use the original pairing account. Installed VHF touchpad access currently requires its administrator token.'
    Write-Output 'No installation, driver operation, firewall change, permission grant or device connection was performed.'
}
