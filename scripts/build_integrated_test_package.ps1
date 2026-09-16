[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repositoryRoot
try {
    & pnpm --filter '@capyio/desktop' build
    if ($LASTEXITCODE -ne 0) { throw 'Shared UI build failed.' }
    & cargo build --offline -p capyio-desktop -p capyio-remote-touchpad-adapter -p capyio-vcamdroid-adapter --bin capyio-desktop --bin capyio-ptp-adb-lab --bin capyio-avc-lab-receiver
    if ($LASTEXITCODE -ne 0) { throw 'Windows application build failed.' }
    $revision = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve source revision.' }
    $dirty = [bool](& git diff --name-only HEAD)
    $stamp = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-ffff')
    $destination = Join-Path $repositoryRoot "target\integrated-test\$stamp"
    New-Item -ItemType Directory -Path $destination | Out-Null
    $files = @('capyio-desktop.exe', 'capyio-ptp-adb-lab.exe', 'capyio-avc-lab-receiver.exe')
    $artifacts = foreach ($name in $files) {
        $source = Join-Path $repositoryRoot "target\debug\$name"
        Copy-Item -LiteralPath $source -Destination $destination
        @{ file = $name; sha256 = (Get-FileHash -LiteralPath (Join-Path $destination $name)).Hash }
    }
    @{
        schemaVersion = 1
        sourceRevision = $revision
        trackedChanges = $dirty
        builtAtUtc = [DateTime]::UtcNow.ToString('o')
        configuration = 'debug-local-test'
        artifacts = @($artifacts)
        prerequisites = @('Windows WebView2', 'Android SDK adb configured through CAPYIO_ADB or ANDROID_HOME or ANDROID_SDK_ROOT', 'One authorized Android device for camera/touchpad', 'Existing CapyIO audio service and drivers for system audio', 'Administrator token for installed VHF touchpad')
        limitations = @('Camera action decodes but does not register a system camera', 'No drivers or Android APK included', 'Not a production installer')
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $destination 'manifest.json') -Encoding UTF8
    Write-Output "Test package: $destination"
    Write-Output "Launch: $(Join-Path $destination 'capyio-desktop.exe')"
}
finally { Pop-Location }
