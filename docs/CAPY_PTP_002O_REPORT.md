# CAPY-PTP-002O Report

Date: 2026-08-30

Status: Android capture to private packet Source boundary complete;
hardware-free validation passes.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

`PrivateTouchpadPacketSource` now forms the Adapter-owned sender boundary after
the Android capture session. It validates initial cancellation and exact source
sequence before returning a fixed-bound `PrivateTouchpadPacketV1`. Encoding and
sequence validation use tentative state and commit only on success.

The Source records accepted packet count, refuses close while the latest frame
retains contacts, closes idempotently after release/cancellation and rejects all
subsequent input. It performs no I/O and grants no transport authority.

## Composed lifecycle evidence

One test drives the CAPY-PTP-002N capture session through:

1. start cancellation;
2. one-finger down;
3. second-finger down;
4. reordered two-finger move;
5. indexed pointer-up;
6. final up/release; and
7. lifecycle-stop cancellation.

All seven frames encode and decode with their complete contact snapshots and
the Source then closes cleanly. Separate tests prove initial-update rejection,
transactional gap recovery, active-contact close refusal, idempotent close and
closed-state rejection.

## Toolchain evidence

The repository has only the reserved `apps/android` directory and no Gradle
project. Inspection found no callable `java`, `gradle`, `kotlinc` or `adb`, and
no `ANDROID_HOME` or `ANDROID_SDK_ROOT`. This slice therefore did not add
unverifiable Kotlin source or install a toolchain.

## Validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_packet_source
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
```

Results: three targeted tests passed, Clippy passed with warnings denied, and
full repository CI passed.

## Files

- `adapters/remote-touchpad/src/source.rs`
- `adapters/remote-touchpad/src/lib.rs`
- `adapters/remote-touchpad/tests/touchpad_packet_source.rs`
- `adapters/remote-touchpad/README.md`
- `platform/android/capyio-host/README.md`
- `docs/plans/completed/0039-android-touchpad-private-packet-source.md`
- architecture, protocol, security, testing, product scope, build status and
  traceability documentation.

## Remaining work

- Kotlin `MotionEvent` copying and Android application lifecycle wiring require
  a buildable Android toolchain and a reviewed project scaffold.
- Authenticated delivery, retry ambiguity and reconnect remain absent.
- No Android component, permission, APK, device or Windows input operation was
  created or executed.
- The worktree remains uncommitted and based on `fc3da36`.
