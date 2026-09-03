# CAPY-PTP-002N Report

Date: 2026-08-30

Status: Android runtime-facing touchpad capture boundary complete; hardware-free
validation passes.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

`AndroidTouchpadCaptureSession` now wraps the existing DTO mapper with the
cross-callback state Android runtime integration needs. It retains at most five
active IDs in a fixed array, accepts motion only while Running and validates
that every action makes a legal transition from the preceding pointer set.

Start emits the epoch/session cancellation barrier. Lifecycle stop emits
cancellation and allows restart on the same sequence stream. Active close emits
cancellation and becomes terminal; repeated stop/close cleanup is bounded and
idempotent. Lifecycle state changes only after the mapper accepts a frame.

## Failure behavior

- `MOVE` cannot introduce, remove or replace an ID.
- `POINTER_DOWN` must retain every active ID and add exactly the indexed ID.
- `POINTER_UP` must include the complete pre-event set and remove the indexed ID.
- `UP` must release the sole active ID.
- malformed lifecycle, mapper validation and cleanup timestamp regression do
  not consume capture state; valid input can recover afterward.
- closed sessions cannot restart or accept motion.

## Validation

```text
cargo test -p capyio-android-host
cargo clippy -p capyio-android-host --all-targets -- -D warnings
```

Results: four new capture tests and four existing mapping tests passed; targeted
Clippy passed with warnings denied. Full repository CI and documentation checks
passed after implementation.

## Files

- `platform/android/capyio-host/src/touchpad_capture.rs`
- `platform/android/capyio-host/src/lib.rs`
- `platform/android/capyio-host/tests/touchpad_capture.rs`
- `platform/android/capyio-host/README.md`
- `docs/plans/completed/0038-android-touchpad-runtime-capture-boundary.md`
- architecture, security, testing, product scope, build status and traceability
  documentation.

## Safety and remaining work

- No JNI/Gradle project, Android component, permission, foreground service,
  notification, APK or physical-device action was added.
- The current API returns frames directly and creates no thread, queue or socket.
- A future Kotlin boundary must copy a bounded complete `MotionEvent` DTO and
  bind actual View/application lifecycle callbacks.
- Authenticated transport and production Runtime scheduling remain absent.
- The worktree remains uncommitted and based on `fc3da36`.
