[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('^S-1-5-21-\d+-\d+-\d+-\d+$')][string]$ControlUserSid,
    [Parameter(Mandatory)][ValidateScript({ $address=$null; [Net.IPAddress]::TryParse($_,[ref]$address) -and $address.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork -and $_ -match '^\d+\.\d+\.\d+\.\d+$' })][string]$LocalIpv4,
    [Parameter(Mandatory)][ValidateScript({ $address=$null; [Net.IPAddress]::TryParse($_,[ref]$address) -and $address.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork -and $_ -match '^\d+\.\d+\.\d+\.\d+$' })][string]$AndroidIpv4,
    [Parameter(Mandatory)][ValidatePattern('^S-1-5-21-\d+-\d+-\d+-\d+$')][string]$ExpectedPreviousUserSid
)

# Targeted repair for the approved local lab. Does not install a driver or
# replace binaries. The existing updater retains the exact previous config.
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$staging = Join-Path $repo 'target/integrated-service-staging/0.5.11'
$service = Get-CimInstance Win32_Service -Filter "Name='CapyIOBroker'"
$expectedExe = Join-Path $staging 'capyio-windows-service.exe'
$oldSid = $ExpectedPreviousUserSid
$userSid = $ControlUserSid
if (-not $service -or -not $service.PathName.StartsWith($expectedExe + ' ', [StringComparison]::OrdinalIgnoreCase) -or
    -not $service.PathName.EndsWith('--display-control-user-sid ' + $oldSid, [StringComparison]::Ordinal)) {
    throw 'The inspected lab service configuration changed; no update applied.'
}
$arguments = @{
    LocalIpv4 = $LocalIpv4
    AndroidIpv4 = $AndroidIpv4
    ServiceExecutable = $expectedExe
    SpeakerBroker = Join-Path $repo 'target/native-service-staging/release/capyio-native-virtual-speaker.exe'
    MicrophoneBroker = Join-Path $repo 'target/native-service-staging/release/capyio-native-virtual-microphone.exe'
    DisplayConfig = Join-Path $staging 'single-display-runtime-config.json'
    DisplayControlUserSid = $userSid
    ExpectedOriginalImagePath = $service.PathName
    EvidenceRoot = Join-Path $repo 'target/evidence/display-account-repair-20260929'
}
& (Join-Path $PSScriptRoot 'update_integrated_capyio_service_lab.ps1') @arguments
