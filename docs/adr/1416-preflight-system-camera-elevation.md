# ADR 1416: Preflight system-camera elevation

Status: accepted

Date: 2026-09-16

## Decision

Before starting any camera helper, the Windows desktop reads its own process
token with `OpenProcessToken` and `GetTokenInformation(TokenElevation)`. When
the current prototype is not elevated, the system-camera Quick Action is
`blocked`, exposes no Start operation and reports
`CAPY.WINDOWS.SYSTEM_CAMERA_ELEVATION_REQUIRED`. Failure to inspect the token is
a separate `CAPY.WINDOWS.SYSTEM_CAMERA_PREFLIGHT_FAILED` condition. An active
owned action is never retroactively hidden by projection logic.

This avoids creating an ADB reverse, camera registration or receiver that can
only fail later when the phone sends its first configuration record. The check
is intentionally local and takes no UI-supplied parameters. It does not grant
privileges, invoke UAC or launch another process.

The desktop adds `windows-sys` as a Windows-only direct dependency with only
Foundation, Security and Threading features. This crate/version already exists
in the workspace and is maintained by the windows-rs project under MIT/Apache
2.0; no new package or network fetch is introduced. Calling `whoami`, shelling
out, or attempting a disposable Global mapping were rejected because they add
process or global-object side effects to a read-only UI preflight.

## Evidence and limitation

Unit coverage forces the non-elevated projection and requires blocked status,
no operation and the stable problem code. The existing physical gate launched
under UAC reached `CAPYIO_SYSTEM_CAMERA_WAITING_FOR_ANDROID`, proving the new
check admits that elevated session. A later Android WebView automation attempt
timed out before phone Start; the bounded Windows test cleaned itself up, so it
is not counted as new media evidence. ADRs 1414 and 1415 retain the successful
live-frame, independent-consumer and reconnect evidence.

Elevation is a temporary prototype requirement, not the desired final design.
A least-privilege background owner should eventually replace elevation of the
entire desktop process.
