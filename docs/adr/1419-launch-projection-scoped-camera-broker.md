# ADR 1419: Launch a projection-scoped camera Broker from the ordinary UI

Status: accepted

Date: 2026-09-16

## Decision

The Windows system-camera Quick Action no longer requires the Tauri/WebView
process itself to be elevated. In an ordinary session, Start invokes Windows
`runas` for the current hash-identical `capyio-desktop.exe` with the fixed mode
`--camera-broker-host` and the ordinary process token's Windows user SID. That
entry point branches before Tauri starts, requires an elevated token, validates
the SID through Windows and exposes only ADR 1418's requester-scoped camera
control pipe. ADR 1420 records why the SID is required for cross-account UAC.

The Broker owns the existing fixed camera session host, AVC receiver, ADB
reverse and cleanup through the same bounded readiness gates as the previously
validated elevated desktop path. The ordinary UI waits at most 60 seconds for
interactive UAC completion, then sends Start over the closed local protocol.
It adopts an already-running Broker and maps only its bounded snapshot into the
shared Quick Action. The WebView cannot supply a path, SID, argument, endpoint,
mapping name or command.

A successful Stop flushes its response and ends this projection-scoped Broker
host. Failed Stop keeps the host reachable for status and cleanup retry. Closing
the UI does not implicitly stop a running projection. An already elevated UI
retains the previously validated in-process compatibility path when no Broker
exists.

This is a least-privilege process split, not a Windows service installation.
No driver, APK, COM registration or Display code changes in this slice. The UAC
launch and live phone-to-camera route still require a separate physical gate
before the old whole-UI elevation limitation can be considered fully retired.

## Evidence

Desktop unit tests map Broker Active/Failed state into bounded Quick Action
DTOs and require ordinary sessions to offer fixed Broker launch rather than a
blocked action. The Broker protocol test verifies that successful Stop returns
before the projection-scoped server exits. Later verification counts are kept
in `BUILD_STATUS.md` and ADR 1420.
