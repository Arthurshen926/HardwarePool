# CAPY-PTP-002C Report

Date: 2026-08-30

Status: reusable Windows Sink session complete; controlled one-finger host API
acceptance and full repository validation pass.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The first explicitly authorized one-finger host test established that the
current Windows API accepts the fixed generic Precision Touchpad fixture. The
experimental CLI now delegates its device/frame lifecycle to a reusable
`SyntheticTouchpadSession` suitable for a later authorized Windows Sink.

This slice did not execute two-, three- or four-finger input, install a driver,
run Android/network traffic or prove visual cursor movement.

## Controlled host evidence

The initial command ran inside the ordinary Codex filesystem/process sandbox:

```text
cargo run -p capyio-windows-input --bin capyio-ptp-inject -- --gesture one-finger-motion --inject --acknowledge-desktop-input
```

The process identity was `DESKTOP-AT8EVE9\CodexSandboxOffline`, at medium
integrity. Device creation succeeded, but the first native submission returned:

```text
injection_status=failed
injection_detail=injection failed: synthetic touchpad batch submission failed: 5
```

This result was treated as a desktop isolation boundary, not evidence that the
encoded gesture was invalid. After the user's existing one-finger authorization
was applied to the same already-built fixed binary outside that sandbox, the
command completed successfully:

```text
schema_version=1
gesture=one-finger-motion
mode=inject
desktop_input_acknowledged=true
frames_projected=11
batches_encoded=9
contact_records_encoded=9
peak_batch_contacts=1
peak_batches_per_frame=1
device_creation=created_and_destroyed
submitted_batches=9
submitted_contact_records=9
input_injected=true
```

Host evidence: Windows display version 25H2, build 26200.9168. A successful
`InjectSyntheticPointerInput` result proves API acceptance of each batch. No
screen capture or independent observer was used, so visible cursor displacement
is not claimed.

## Reusable Sink lifecycle

`SyntheticTouchpadSession` now:

- constructs a validated `WindowsTouchpadProjector` before creating the native
  device;
- owns one projector/device pair;
- accepts complete `TouchpadFrame` snapshots and reports exact submitted batch
  and contact counts;
- exposes explicit epoch advancement;
- cancels retained contacts on explicit close;
- attempts bounded cancellation after a native submission failure and marks the
  session failed;
- rejects additional input after failure or close; and
- attempts best-effort cancellation when an active session is abandoned, before
  the existing RAII device owner destroys the native handle.

Projection remains transactional. A contract-invalid frame does not poison the
session or consume the rejected sequence. The session owns no network,
authorization, pairing, reconnect or UI policy.

## Automated evidence

The new fake-device tests cover:

- explicit close cancellation and post-close rejection;
- primary injection failure plus successful cancellation;
- retained primary and cleanup failures;
- epoch-change cancellation;
- abandoned-session drop cancellation; and
- transactional recovery after a projection error.

These tests cannot load user32 or inject desktop input. The refactored CLI
continues to default to dry-run and retains both explicit native-injection
gates.

## Files

- `platform/windows/capyio-input/src/session.rs`
- `platform/windows/capyio-input/src/projection.rs`
- `platform/windows/capyio-input/src/lib.rs`
- `platform/windows/capyio-input/src/bin/capyio-ptp-inject.rs`
- `platform/windows/capyio-input/README.md`
- `docs/plans/completed/0027-synthetic-touchpad-sink-session.md`
- `docs/ARCHITECTURE.md`
- `docs/SECURITY_MODEL.md`
- `docs/TESTING.md`
- `docs/REQUIREMENTS_TRACEABILITY.md`

## Dependency note

No dependency or Cargo feature was added.

## Validation

The following no-injection checks pass:

```text
cargo fmt --all -- --check
cargo check -p capyio-windows-input --all-targets
cargo clippy -p capyio-windows-input --all-targets -- -D warnings
cargo test -p capyio-windows-input
cargo run -q -p capyio-windows-input --bin capyio-ptp-inject -- --gesture one-finger-motion --dry-run
cargo xtask validate-docs
cargo xtask ci
```

The Windows input crate passed ten library tests, three CLI parser tests, three
fixture tests and five projection tests. The first full CI attempt encountered
an unrelated existing Audio Share test failing to bind a local TCP socket with
Win32 error 10013. That exact test passed on immediate isolated rerun, and the
subsequent complete `cargo xtask ci` passed formatting, workspace check/Clippy/
tests, demo, docs, manifests, Adapter smoke, repository validation and frontend
typecheck/build. No code was changed to mask the transient failure.

## Remaining work and risks

- The user-mode API is currently preferable to VHF because device creation and
  one-finger submission both succeed, but it remains a pre-release Windows API.
- Visual one-finger motion and native two-finger scrolling still need observed
  acceptance evidence.
- Three/four-finger raw-contact behavior must be checked against the current
  Windows touchpad settings; direct `InjectTouchpadAction` remains an explicit
  lab/fallback topic, not sender-side gesture policy.
- Android runtime capture, bounded transport, authorization and Windows Runtime
  wiring are not implemented by this slice.
- The worktree remains uncommitted and based on `fc3da36`; integration with the
  newer `main` still requires separate review and authorization.

## References

- Microsoft Precision Touchpad programming guide:
  <https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/precision-touchpad-guide>
- Microsoft `CreateSyntheticPointerDevice2`:
  <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-createsyntheticpointerdevice2>
- Microsoft synthetic device creation options:
  <https://learn.microsoft.com/en-us/windows/win32/api/winuser/ne-winuser-synthetic_device_creation_options>
