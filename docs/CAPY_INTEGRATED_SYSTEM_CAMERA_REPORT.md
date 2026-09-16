# Integrated system-camera action: implementation gate

Date: 2026-09-16

## Delivered

ADR 1412 adds a fixed Windows Quick Action that composes the existing camera
session host and AVC-to-shared-NV12 receiver. Users no longer need to launch
these two helpers manually for this action. Decode-only reception remains a
separate, mutually exclusive action. The package includes four executables;
diagnostics checks the host's presence alongside the other helpers.

Readiness is exact and bounded to ten seconds. Stop requests normal camera
cleanup by closing stdin, waits three seconds, and reports forced termination
if required. A failed receiver start rolls back the camera. Action polling
detects either process ending and attempts cleanup of its companion. This is
local lab process ownership, not a completed headless production supervisor.

## Validation

- `cargo test -p capyio-desktop --lib --offline`: 68 passed, 7 physical tests
  intentionally ignored.
- `cargo clippy -p capyio-desktop --all-targets --offline -- -D warnings`: passed.
- Tests cover exact bounded readiness, normal EOF shutdown, invalid-readiness
  rollback, premature host failure, and fixed publish-vs-decode arguments.
- Tests launch inert PowerShell fixtures, not virtual cameras or drivers.
- `scripts/build_integrated_test_package.ps1`: passed shared Vue typecheck/build
  and all four Windows binary builds; generated
  `target/integrated-test/20260916-130013-5912` with per-file SHA-256 manifest.
  This is explicitly a dirty development build, not an exact-commit release.
- `cargo xtask validate-docs`, `cargo xtask validate-manifests` and
  `git diff --check`: passed.

## Physical prerequisite and remaining gate

Read-only preflight found the authorized Android debugging device online, but
the expected `C:\ProgramData\CapyIO\Lab\Camera\capyio_windows_camera_mf.dll`
was absent and the fixed HKLM COM registration was not found in the queried
registry view. No COM component was installed in this implementation slice.

Therefore no ordinary Windows camera-client live-frame success is claimed.
Next: review and hash-pin the COM deployment candidate and rollback, deploy on
the authorized lab host, then exercise this action with the unified Android
camera and an ordinary Windows consumer. Validate live frames (not the offline
placeholder), Stop, receiver loss, retry and removal of the session device.
Global shared-memory privileges must also be checked in the actual user token.

Display development and the existing audio/driver installation are unchanged.
