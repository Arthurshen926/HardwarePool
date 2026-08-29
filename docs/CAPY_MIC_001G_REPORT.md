# CAPY-MIC-001G — Trusted MicYou host configuration report

Date: 2026-08-29

Status: completed in PR #14; release qualification remains separate

## Outcome

CapyIO Desktop no longer requires MicYou environment variables on every normal
launch. After one explicit host-side provisioning step it automatically reads a
schema-v1 file from the fixed user-local CapyIO directory and creates the same
Runtime-owned microphone Quick Action implemented by CAPY-MIC-001F.

The implementation deliberately does not guess the microphone ingress from a
friendly name. Windows audio names may be duplicated or localized. The host
tool instead probes the separately supplied patched MicYou executable, selects
an operator-provided exact stable endpoint ID and derives the expected name
from the bounded inventory. It never persists the temporary device index.

## Configuration boundary

The default Windows path is:

```text
%LOCALAPPDATA%\CapyIO\host\micyou-v1.json
```

The JSON parser has a 16 KiB limit, a required schema version and unknown-field
rejection. Existing files are opened with create-new behavior and are not
silently overwritten. A complete `CAPYIO_MICYOU_*` environment set remains an
explicit development override; any partial override fails closed rather than
mixing file and environment values.

The file contains a local executable path and raw endpoint identity, so it is
owned by the trusted host boundary. Custom `Debug` output redacts those values.
The Tauri/WebView DTO contains neither; it exposes only the validated bind
address and port as Android connection guidance.

## Host tool

The new `capyio-micyou-config` binary provides two commands:

- `provision` verifies the pinned MicYou version and
  `device-stable-id-v1` capability, reads the bounded endpoint inventory,
  resolves the exact stable ID, derives the expected name and creates the fixed
  file;
- `validate` reloads the trusted configuration and repeats the current
  executable/version/capability/endpoint checks without starting the audio
  server.

Neither command downloads or redistributes MicYou. Driver/APK installation and
Android permission changes remain outside this boundary.

## Verification

The following passed on 2026-08-29:

- five `capyio-micyou-host-config` unit tests;
- 31 Tauri desktop library tests, with four physical tests explicitly ignored;
- all-target checks for the new host crate and desktop;
- targeted Clippy with warnings denied;
- repository structural, documentation and manifest validation;
- `cargo xtask ci`, including workspace check/Clippy/tests, demos, Adapter
  smoke, repository validation and frontend typecheck/build.

Tests cover fixed-path derivation, complete/partial override behavior, exact-ID
selection with duplicate endpoint names, schema/unknown-field rejection,
round-trip persistence, overwrite refusal, debug redaction and serialized
Quick Action privacy.

## Controlled-lab follow-up

A separately authorized CAPY-MIC-001H step provisioned the real fixed-path
configuration and validated it against the current pinned local CLI and stable
microphone-ingress endpoint. The desktop Quick Action then loaded that file and
completed start, physical PCM, disconnect, retry and stop without environment
overrides. The executable path and raw endpoint identity remained absent from
the WebView DTO and ordinary report output.

That follow-up did not install a driver/APK, change Android permissions,
restart Windows audio services, alter boot settings or reboot the host. It did
add narrowly scoped local-lab firewall rules as recorded in CAPY-MIC-001H. No
commit, push or pull request was created.

## Remaining work

1. Continue Android lock/background, permission-revocation, reboot, soak,
   packaging and legal/distribution qualification.
