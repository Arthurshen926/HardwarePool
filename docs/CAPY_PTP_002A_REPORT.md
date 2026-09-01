# CAPY-PTP-002A Report

Date: 2026-08-30

Status: Android/Windows frame mapping complete; native input submission and
physical gesture acceptance pending.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The direction-neutral `TouchpadFrame` contract now has deterministic platform
boundary mappings on both sides of the planned phone-to-Windows path:

- `capyio-android-host` maps a narrow `MotionEvent` DTO into post-event complete
  touchpad snapshots; and
- `capyio-windows-input` differences those snapshots into fixed-capacity
  PT_TOUCHPAD batches and encodes inspectable `POINTER_TYPE_INFO` structures.

No JNI, Gradle project, manifest, permission, APK, network path, synthetic
device creation, input injection, driver or physical device operation was
performed in this slice.

## Android mapping

`AndroidTouchpadMapper`:

- requires one explicit `cancel_all` at stream/epoch start;
- accepts `Down`, `PointerDown`, `Move`, `PointerUp`, `Up` and `Cancel`;
- omits the Android action index from the post-event snapshot on pointer-up/up;
- preserves Android pointer IDs even when pointer-array order changes;
- maps local finite pixel coordinates to the declared himetric surface;
- accepts finger tools only and a maximum of five pointers;
- maps optional pressure to the Profile's `u16` normalized magnitude and clamps
  calibrated Android pressure above 1 to full scale;
- rejects malformed action indexes, duplicate IDs, out-of-surface coordinates,
  non-finger tools and timestamp regression without consuming sequence state;
  and
- treats `ACTION_CANCEL` as fail-safe cleanup and ignores its pointer payload
  after enforcing the list bound.

The current Android touch-area DTO deliberately declares a non-clickable
surface and does not guess an axis-aligned rectangle from MotionEvent
major/minor/orientation values.

## Windows projection

`WindowsTouchpadProjector`:

- is allocation-free after construction and retains at most five contacts;
- sorts contact IDs deterministically while accepting reordered snapshots;
- submits active contacts plus one-shot releases for contacts absent from the
  next complete snapshot;
- emits cancelled records immediately on `cancel_all`, a sequence gap or epoch
  advance and suppresses later updates until the contract cancellation barrier
  is satisfied;
- splits a simultaneous five-for-five replacement into a release batch and an
  active batch, so every batch stays at or below five contacts and every frame
  stays at or below two batches;
- derives synthetic device contact count and physical dimensions directly from
  the validated Profile descriptor; and
- retains optional size/pressure in the projected DTO without copying them into
  pixel-defined native contact fields.

On Windows, `NativeTouchpadBatch::encode` uses PT_TOUCHPAD,
`ptHimetricLocation` and `ptHimetricLocationRaw`. Active contacts use
in-range/in-contact/confidence flags, ordinary releases clear range/contact,
and abrupt cleanup adds the cancelled flag. Pixel locations, Win32 `dwTime`,
QPC and touch masks remain zero; the Android uptime clock is not relabeled as a
Windows clock.

Integrated click-pad/pressure-pad descriptors are rejected until their native
button semantics are documented and physically accepted. This does not block
the phone touch-area path, whose tap and multi-contact motion are intended to
be recognized by Windows from raw contacts.

## Validation

These commands completed successfully:

```text
cargo fmt --all -- --check
cargo check -p capyio-android-host -p capyio-windows-input --all-targets
cargo clippy -p capyio-android-host -p capyio-windows-input --all-targets -- -D warnings
cargo test -p capyio-android-host -p capyio-windows-input
cargo xtask validate-docs
cargo xtask ci
git diff --check
```

The Android suite passed four tests. The Windows projection suite passed five
tests in addition to the four existing API-probe unit tests. Full repository CI
passed formatting, workspace check/Clippy/tests, fixture demo, documentation,
manifests, Adapter smoke, structural validation and frontend typecheck/build.

## Files

- `platform/android/capyio-host/Cargo.toml`
- `platform/android/capyio-host/README.md`
- `platform/android/capyio-host/src/lib.rs`
- `platform/android/capyio-host/src/touchpad.rs`
- `platform/android/capyio-host/tests/touchpad_mapping.rs`
- `platform/windows/capyio-input/Cargo.toml`
- `platform/windows/capyio-input/README.md`
- `platform/windows/capyio-input/src/lib.rs`
- `platform/windows/capyio-input/src/projection.rs`
- `platform/windows/capyio-input/tests/touchpad_projection.rs`
- `docs/adr/0043-project-touchpad-snapshots-through-bounded-platform-dtos.md`
- `docs/plans/completed/0025-touchpad-platform-frame-mapping.md`
- `docs/ARCHITECTURE.md`
- `docs/TESTING.md`
- `docs/REQUIREMENTS_TRACEABILITY.md`

## Dependency note

`capyio-android-host` adds the existing internal workspace crate
`capyio-input`. `capyio-windows-input` enables the Pointer, Controls and
WindowsAndMessaging feature modules on the already pinned `windows-sys 0.61.2`
dependency. No external package or version was added.

## Unresolved evidence and risks

This slice does not prove:

- real Android view event capture, JNI copying, lifecycle or networking;
- Windows injection success for any encoded batch;
- one-finger cursor/tap, two-finger pan/zoom or native three/four-finger system
  gestures;
- Settings visibility/configuration, RDP, multi-user, sleep/resume or crash
  cleanup;
- click-pad/pressure-pad button injection or native optional contact geometry;
- production reconnect cancellation; or
- VHF/HID behavior.

Microsoft currently marks the new Precision Touchpad APIs as pre-release. The
next controlled slice is `CAPY-PTP-002B`: add an explicit, non-default local
injection harness around these encoded batches, first verify bounded
one/two-contact behavior, then separately run three/four-finger acceptance with
user-visible safeguards and retained results.

## Integration risk

The worktree is still based on `fc3da36`, while public `main` was observed at
`d4df224`; integration requires a separately reviewed merge/rebase. Preserved
uncommitted generic touch-to-pointer work also remains in this worktree.

No commit, merge, rebase, push or pull request was performed.
