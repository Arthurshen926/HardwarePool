# CAPY-PTP-000 Report

Date: 2026-08-30

Status: user-mode API feasibility slice complete; gesture acceptance pending.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The identified Windows host can resolve the documented synthetic Precision
Touchpad API and create then immediately destroy a generic five-contact
synthetic touchpad without installing a driver or injecting input.

The implementation in `platform/windows/capyio-input`:

- loads only the System32 copy of `user32.dll`;
- dynamically resolves `CreateSyntheticPointerDevice2`,
  `InjectSyntheticPointerInput`, `InjectTouchpadAction` and
  `DestroySyntheticPointerDevice` because the local SDK headers lag the
  runtime API;
- validates `1..=5` contacts and non-zero himetric physical dimensions before
  any native call;
- keeps the native declarations and audited unsafe calls inside the Windows
  platform crate;
- provides a read-only symbol probe by default;
- requires `--create-device` for the bounded create/destroy smoke; and
- never submits contact frames or gesture actions in this slice.

ADR 0041 records the decision to evaluate this user-mode path before a VHF
driver. `capyio-core` and `capyio-input` remain unchanged by this slice.

## Identified environment

```text
Environment OS version: 10.0.26200.0
user32.dll product/file version: 10.0.26100.8875
Installed Windows SDK headers: 10.0.26100.0
```

The installed SDK's `WinUser.h` declares `PT_TOUCHPAD` and
`POINTER_FEEDBACK_NONE`, but a repository search found no declaration for
`CreateSyntheticPointerDevice2`, `InjectTouchpadAction` or the new creation
parameter/option types. Runtime discovery is therefore required on this host.

## Probe evidence

The symbol-only command produced:

```text
schema_version=1
platform=windows
user32_loaded=true
user32_load_error=none
export.CreateSyntheticPointerDevice2=true
export.InjectSyntheticPointerInput=true
export.InjectTouchpadAction=true
export.DestroySyntheticPointerDevice=true
synthetic_touchpad_api_available=true
device_creation=not_requested
```

The explicit create-device command produced:

```text
schema_version=1
platform=windows
user32_loaded=true
user32_load_error=none
export.CreateSyntheticPointerDevice2=true
export.InjectSyntheticPointerInput=true
export.InjectTouchpadAction=true
export.DestroySyntheticPointerDevice=true
synthetic_touchpad_api_available=true
device_creation=created_and_destroyed
device_max_contacts=5
device_width_himetric=10000
device_height_himetric=6000
input_injected=false
```

No pointer address, device identifier, user data or input payload is retained.

## Validation

These commands completed successfully:

```text
cargo fmt --manifest-path platform/windows/capyio-input/Cargo.toml -- --check
cargo check -p capyio-windows-input --all-targets
cargo clippy -p capyio-windows-input --all-targets -- -D warnings
cargo test -p capyio-windows-input
cargo run -p capyio-windows-input --bin capyio-ptp-probe -- --symbols-only
cargo run -p capyio-windows-input --bin capyio-ptp-probe -- --create-device
cargo xtask validate-docs
cargo xtask ci
```

The focused unit suite passed four tests covering default parameters, contact
and size rejection, exact required-symbol availability and fail-fast invalid
parameters. Full repository CI also passed Rust format/check/Clippy/tests,
fixture demo, documentation, manifests, Adapter smoke, structural validation
and frontend typecheck/build.

## Files

- `platform/windows/capyio-input/Cargo.toml`
- `platform/windows/capyio-input/README.md`
- `platform/windows/capyio-input/src/lib.rs`
- `platform/windows/capyio-input/src/bin/capyio-ptp-probe.rs`
- `docs/adr/0041-probe-user-mode-synthetic-precision-touchpad-before-vhf.md`
- `docs/plans/completed/0023-precision-touchpad-user-mode-probe.md`
- `docs/PRODUCT_REQUIREMENTS.md`
- `docs/ARCHITECTURE.md`
- `docs/TESTING.md`
- `Cargo.lock`

## Dependency note

The Windows platform crate now enables `Win32_Foundation` and
`Win32_System_LibraryLoader` on the already pinned workspace dependency
`windows-sys 0.61.2`. No new package, source import or network fetch was added.

## Unresolved evidence and risks

This result proves API discovery and handle lifecycle only. It does not prove:

- single-finger pointer motion/click or two-finger pan/zoom;
- Windows-native three/four-finger system gestures;
- Settings-page configuration/natural-scroll behavior;
- RDP, multi-user, sleep/resume or process-crash cleanup;
- Android `MotionEvent` to a direction-neutral `TouchpadFrame`;
- network transport, Route epochs or disconnect release;
- VHF/HID enumeration or driver compatibility.

Microsoft's documentation is still marked pre-release, so symbol and creation
success must not be promoted into a stable compatibility claim. The next safe
slice is `CAPY-PTP-001`: define and fixture-test the direction-neutral,
five-contact touchpad Profile before any input is injected.

## Integration risk

The dedicated worktree exists, but its base commit is not the current public
`main`: `main` is at `d4df224`, while this worktree is based on `fc3da36`.
`fc3da36` contains the video/input contract baseline and is not currently an
ancestor of `main`. The worktree also already contained an uncommitted
touch-to-pointer fallback slice before `CAPY-PTP-000` began. Those changes were
preserved and are not evidence for Precision Touchpad behavior.

Before integration, the common contract baseline and this worktree must be
reconciled with current `main` in a reviewed merge/rebase operation. No commit,
merge, rebase, push or pull request was performed in this slice.
