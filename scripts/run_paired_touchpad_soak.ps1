[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][guid]$NodeId,
    [Parameter(Mandatory=$true)][ipaddress]$PeerAddress,
    [Parameter(Mandatory=$true)][ValidateRange(1,65535)][int]$ControlPort,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$ExpectedExeSha256
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $repo 'target\debug\capyio-desktop.exe'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
if (-not ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Use the original pairing account with its elevated token; no driver will be installed.'
}
if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ine $ExpectedExeSha256) {
    throw 'Desktop build changed. Inspect the new build before starting physical input.'
}
$evidence = Join-Path $repo ('target\lab-evidence\touchpad-soak-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-ffff'))
New-Item -ItemType Directory -Path $evidence | Out-Null
$started = [DateTime]::UtcNow
$child = Start-Process -FilePath $exe -ArgumentList @('--verify-paired-touchpad-soak', $NodeId.ToString(), $PeerAddress.ToString(), $ControlPort.ToString(), '--acknowledge-desktop-input') -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $evidence 'stdout.txt') -RedirectStandardError (Join-Path $evidence 'stderr.txt')
$null = $child.Handle
$timedOut = -not $child.WaitForExit(600000)
if ($timedOut) { $child.Kill(); $child.WaitForExit() }
$child.Refresh()
[ordered]@{ account=$identity.Name; sid=$identity.User.Value; exeSha256=$ExpectedExeSha256; startedAtUtc=$started.ToString('o'); endedAtUtc=[DateTime]::UtcNow.ToString('o'); timedOut=$timedOut; exitCode=$child.ExitCode; inputRequiresVisibleOwnerConsent=$true } |
    ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidence 'result.json') -Encoding UTF8
if ($timedOut -or $child.ExitCode -ne 0) { exit 1 }
