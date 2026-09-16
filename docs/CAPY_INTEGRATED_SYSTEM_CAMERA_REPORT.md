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

## Phone live-frame and independent-consumer gate

The composed Quick Action was then exercised against unified Android 0.5.6.
A normal-user diagnostic run proved the actual remaining boundary: Android
captured Camera ID 0, but the receiver exited with `CreateFileMappingW` error 5.
This is now surfaced as
`CAPY.WINDOWS.SYSTEM_CAMERA_ELEVATION_REQUIRED` instead of a generic receiver
failure.

Under an explicitly elevated user session, the final bounded gate passed:

- Android Quick Action Start activated the Camera2 foreground service.
- The production Global ingress delivered phone frame sequence 3 at timestamp
  66,666,000 ns, with 1,382,400 decoded NV12 bytes and checksum 112,335,401.
- A separate Media Foundation process enumerated the current CapyIO system
  camera, activated it and read a 1,382,400-byte sample while the live mapping
  existed (`external_consumer=pass`). This is distinct from the earlier
  generated-placeholder test.
- The composed Windows Stop succeeded. Android Stop then left
  `Active Camera Clients: []`; `adb reverse --list` was empty; camera preflight
  reported `existing_registration=false`.
- No pixels or video were retained. The evidence log contains bounded metadata
  only under ignored `target/test-results`.
- The post-fix shared UI/test-package build passed and generated
  `target/integrated-test/20260916-133057-3536`; its manifest marks the dirty
  development source and hashes all four shipped executables.

This closes the short phone-to-system-camera functional gate. Receiver-loss
reconnect was then exercised without restarting the Windows action. The first
phone stream reached publication sequence 1, Android Stop produced a confirmed
quiet interval, and Android Start recovered delivery at sequence 1027,
timestamp 34,199,658,000 ns, 1,382,400 bytes and checksum 112,599,881. The
independent Media Foundation consumer passed after recovery. Final cleanup again
left no phone camera client, ADB reverse or temporary camera registration. ADR
1415 records this gate and the added drop-time child cleanup.

The post-reconnect shared UI/test-package build passed and generated
`target/integrated-test/20260916-134207-6490`; the manifest records its dirty
development revision and hashes the four shipped executables.

Long-duration operation, Windows privacy UI and several ordinary third-party
camera applications remain broader acceptance work. The current prototype also
requires an elevated Windows process for this action; ADR 1414 records why a
least-privilege broker is still future work.

ADR 1416 moves this constraint into a read-only preflight. A normal Windows
session now sees the action blocked with no Start operation and a stable
elevation-required problem before any ADB mapping or helper is created. The
same ignored gate under UAC passed the check and reached its Android-waiting
marker. This changes failure timing and UX only; it does not claim a privilege
broker or automatic elevation.

The post-preflight shared UI/test-package build passed and generated
`target/integrated-test/20260916-134829-1009`; its manifest records the dirty
development revision and hashes all four shipped executables.

ADR 1417 removes the earlier process-spawn-only status. Camera receiver Start
now waits for its exact bounded listener readiness before the unified desktop
reports Active; failure reaps the process and ADB reverse. A physical
decode-only Start/Stop passed and left no reverse mapping. This strengthens
lifecycle truthfulness but remains distinct from the live-frame gates above.

ADR 1418 establishes the first least-privilege Broker slice: a separate fixed,
owner-scoped camera control pipe, closed Status/Start/Stop protocol and
hardware-independent lifecycle runtime. Its tests include actual named-pipe
round trip, idempotency, partial-start rollback and unexpected exit. The real
camera session host and receiver have not moved behind it, and no service is
installed, so the desktop elevation limitation remains.

The post-readiness shared UI/test-package build passed and generated
`target/integrated-test/20260916-141529-8853`; its manifest records the dirty
development revision and hashes all four shipped executables.

Display development and the existing audio/driver installation are unchanged.
