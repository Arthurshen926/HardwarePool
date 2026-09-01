# CAPY-PTP-002A — Android and Windows touchpad frame mapping

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-PLAT-004`, `FR-CAP-001..006`,
`NFR-STAB-001..004`, `NFR-SEC-003`, `NFR-MAINT-001..003`

## Objective

Implement deterministic Android `MotionEvent` DTO to `TouchpadFrame` mapping
and `TouchpadFrame` to bounded Windows PT_TOUCHPAD native-batch encoding without
performing JNI, device creation or desktop input injection.

## In scope

- narrow Android action/pointer/surface DTOs and a stateful frame mapper;
- exact up/action-index, cancel, stable-ID, coordinate, pressure and timestamp
  behavior;
- allocation-free Windows complete-snapshot differencing and cancellation;
- at most two Windows batches of at most five contacts each;
- Windows `POINTER_TYPE_INFO` encoding with PT_TOUCHPAD and himetric positions;
- synthetic tests for Android lifecycles, Windows release/gap/epoch behavior
  and native structure fields;
- ADR 0043, platform README and testing documentation.

## Out of scope

- Kotlin/JNI/Gradle, Android manifest, permissions, service or APK;
- calling `InjectSyntheticPointerInput` or `InjectTouchpadAction`;
- integrated physical-button/native contact-area mapping;
- network transport, pairing, jitter buffer or reconnect implementation;
- physical cursor, click, pan/zoom or three/four-finger acceptance;
- VHF/HID driver work.

## Acceptance criteria

1. Android down/move/pointer-down/pointer-up/up/cancel maps to post-event
   complete snapshots with stable IDs and bounded physical coordinates.
2. Invalid action indexes, duplicate IDs, non-finger tools, non-finite or
   out-of-surface values and timestamp regression fail transactionally.
3. Windows projection emits one-shot releases, explicit cancels and no stale
   contacts after a sequence gap or epoch advance.
4. Simultaneous contact replacement never produces a batch over five contacts
   and uses at most two batches.
5. Native encoding uses PT_TOUCHPAD, documented pointer flags and himetric
   coordinates while leaving cross-clock timestamps zero.
6. No input injection or device mutation occurs and full repository CI passes.

## Required validation

```text
cargo fmt --all -- --check
cargo check -p capyio-android-host -p capyio-windows-input --all-targets
cargo clippy -p capyio-android-host -p capyio-windows-input --all-targets -- -D warnings
cargo test -p capyio-android-host -p capyio-windows-input
cargo xtask validate-docs
cargo xtask ci
```

## Dependency changes

`capyio-android-host` adds the internal workspace crate `capyio-input` to map
the standard Profile. `capyio-windows-input` enables the existing pinned
`windows-sys` Pointer/Controls/WindowsAndMessaging features. No external
package or version is added.

## Safety

This slice compiles and inspects native structures only. It does not call an
injection function, create a device, install an APK/driver, change permissions
or perform Git integration/publishing operations.

## Implementation plan

1. Record ADR 0043 and the bounded DTO shapes.
2. Implement and test Android post-event snapshot mapping.
3. Implement and test Windows stateful projection/native encoding.
4. Update platform docs, run full CI and retain a completion report.

## Completion record

Implemented:

- Android initial-cancel barrier and post-event MotionEvent DTO mapping;
- bounded finger/coordinate/pressure/action/timestamp validation;
- allocation-free Windows snapshot differencing and cleanup state;
- fixed one/two-batch projection for active/released/cancelled contacts;
- PT_TOUCHPAD native structure encoding with himetric locations;
- focused Android and Windows tests plus ADR/platform/normative documentation.

Validation:

- focused format/check/Clippy/tests: pass;
- Android mapping tests: 4 pass;
- Windows projection tests: 5 pass;
- existing Windows API probe tests: 4 pass;
- `cargo xtask validate-docs`: pass;
- full `cargo xtask ci`: pass.

Not validated:

- real Android runtime/JNI/APK behavior;
- Windows input submission or any physical gesture;
- integrated button and optional native contact-area mapping;
- VHF/driver behavior.

Detailed evidence: `docs/CAPY_PTP_002A_REPORT.md`.
