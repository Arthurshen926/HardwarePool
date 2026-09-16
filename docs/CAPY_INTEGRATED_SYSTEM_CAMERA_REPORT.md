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

## Initial physical prerequisite

Read-only preflight found the authorized Android debugging device online, but
the expected `C:\ProgramData\CapyIO\Lab\Camera\capyio_windows_camera_mf.dll`
was absent and the fixed HKLM COM registration was not found in the queried
registry view. No COM component was installed in this implementation slice.

This initial missing-deployment finding was resolved by the follow-up below.

## Follow-up: approved COM deployment and system-consumer gate

The current Release COM DLL was built offline and deployed through Windows UAC
to the fixed lab target with SHA-256
`E03BAA67CC9E5D080C282AE9B54E1F54D73E8B70993800EDFCEA4B9643D5A46D`.
The deployment exited 0; subsequent read-only preflight verified the candidate
hash, deployed hash and fixed COM server path (`deployed=True`,
`registered=True`). The wrong-hash preflight failed with exit 1. ADR 1413 records
the fixed target, hash-scoped rollback and cleanup protections.

In the actual user's non-administrator session:

- The new ignored test
  `physical_deployed_session_starts_and_stops_via_control_pipe` passed: the
  desktop owner observed exact readiness and normal Stop/Shutdown.
- `capyio-camera-virtual-lab roundtrip` passed, enumerating exactly one camera
  and reading a 1,382,400-byte generated NV12 sample.
- `capyio-camera-virtual-lab shared-roundtrip` passed for two independent
  enumerating/activating consumer processes. Both read generated samples of
  1,382,400 bytes with nominal 333,333 (100 ns) duration. Despite its name,
  this command tests shared camera consumption, not Global ingress publication.
- Cleanup passed; subsequent `preflight` reported
  `existing_registration=false` after roundtrip.
- Default desktop tests: 68 passed, eight physical tests ignored. Strict
  desktop all-target Clippy and documentation validation passed.

The COM component intentionally remains installed for the next lab test;
temporary camera sessions were closed. No driver, APK or FrameServer restart
was performed. No captured phone pixels or recordings were retained.

## Remaining gate

No ordinary Windows camera-client **phone live-frame** success is claimed.
Next exercise the integrated action with the unified Android camera and a
consumer, distinguishing live frames from generated fallback. Validate Global
ingress permission, Stop, receiver loss, retry and session cleanup. The tests
above do not establish Global shared-memory publication privileges.

Display development and the existing audio/driver installation are unchanged.
