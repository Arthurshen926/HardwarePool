# CAPY-MIC-000A — MicYou audit and external-process Adapter

Status: complete

Owner: Codex and project owner

Created: 2026-08-28

Completed: 2026-08-28

Depends on: `CAPY-AUDIO-CORE-001`

## Objective

Pin and audit MicYou, then implement the smallest bounded CapyIO-owned process
boundary without importing GPL code or claiming a Windows virtual microphone.

## Completed scope

1. Pinned MicYou v2.0.1 and exact commit, license, source archive and official
   release artifact hashes.
2. Recorded the private TCP/UDP, PCM/Opus, FEC/jitter, CLI and Windows output
   boundary findings in ADR 0036 and the third-party manifest.
3. Added exact-version and bounded output-device probes.
4. Added explicit Wi-Fi bind/port/device launch arguments without a shell.
5. Added bounded child-output draining, startup deadline, status, stop, reap and
   Drop cleanup.
6. Mapped this AdapterManaged path to the common VoiceInteractive policy.
7. Tested the full process boundary against a repository-built fixture.

## Retained non-scope

- copying or linking MicYou code/schema/crates;
- building or executing real MicYou in this completion claim;
- installing the MicYou APK or VB-CABLE;
- a CapyIO Windows virtual microphone/PCM ingress;
- UI/Runtime commands and physical phone-to-application recording evidence.

## Dependency changes

No new third-party production dependency. The Adapter uses `capyio-audio`,
`thiserror` and the Rust standard library already present in the workspace.

## Safety

No driver, APK, permission, boot-policy or signing change was performed.

## Validation

See `docs/CAPY_MIC_000A_REPORT.md`.

## Follow-up Gate

`CAPY-MIC-001` must define and implement the CapyIO virtual capture endpoint and
bounded user-mode PCM ingress before real end-to-end validation.
