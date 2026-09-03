# CAPY-CAMERA-001C14 — Closed Windows live-hold orchestration

Date: 2026-08-30

Status: implementation and hardware-free validation complete; controlled
system execution remains pending separate approval.

## Objective

Reduce the error-prone manual ordering between the AVC receiver, decoded-frame
Global mapping and fixed virtual-camera hold without adding a product service,
caller-selected process path, variable port, persistent registration or hidden
privilege elevation.

## Implementation

- `capyio-camera-virtual-lab live-hold` accepts no additional argument.
- Preflight rejects an existing `Global\\CapyIO.CameraIngress.v1` mapping or an
  existing CapyIO Camera registration instead of adopting ambient state.
- The command resolves only `capyio-avc-lab-receiver.exe` beside its own
  executable and supplies exactly `--max-access-units 3600 --publish-shared`.
- It waits at most 30 seconds for a validated mapping header before invoking the
  existing Session/CurrentUser registrar.
- During the fixed 60-second hold it checks receiver liveness and mapping
  validity every 100 ms. Receiver exit, mapping loss, duplicate registration or
  readiness timeout is explicit failure.
- Every path invokes registrar Stop/Shutdown, terminates and reaps the receiver,
  then requires the production mapping to disappear within five seconds.

The command does not install/update the Android APK, start Android capture,
establish ADB reverse, deploy/remove the COM DLL, accept an ADB target, launch a
camera consumer, retain pixels or elevate itself. Those remain explicit outer
lab actions.

## Evidence

- The Windows binary unit suite covers the new closed command token, fixed
  receiver argument vector and the exact not-found mapping retry rule. Access
  denied and invalid layout fail immediately.
- `cargo test --package capyio-windows-camera-mf --bin
  capyio-camera-virtual-lab` passed all five tests.
- `cargo clippy --package capyio-windows-camera-mf --bin
  capyio-camera-virtual-lab -- -D warnings` passed.
- Repository structural validation, `git diff --check` and the full
  `cargo xtask ci` passed after the documentation and hash-pin update.
- The release artifacts prepared for a later controlled run are:
  - receiver SHA-256
    `ED32E0A4D7117E07706A6FF04DD5730E17DD2D984EAF06AA1C74AA1ECF89ADD8`;
  - orchestration executable SHA-256
    `4E4F73ADC102FAD15C9E3A24293F4AD34CB0A6C92AC757B86A691E560F5FFF0D`;
  - COM DLL SHA-256
    `F467E22FEA1CA830E7C7F2F7047BA2BB7ECC9F94021784754BBDB95D1294348E`.
    The deployment script now rejects any other DLL hash. Historical C7/C8
    reports retain the older hash they actually exercised.

## Remaining controlled evidence

A real `live-hold` run creates a temporary Windows system virtual camera and
requires the hash-locked COM deployment plus elevated Global mapping ownership.
It was deliberately not run under the general development instruction. A later
exact approval should retain the deployed DLL hash, ADB target/reverse mapping,
command and rollback checks, and confirm no registration, mapping, process or
camera client remains afterward.
