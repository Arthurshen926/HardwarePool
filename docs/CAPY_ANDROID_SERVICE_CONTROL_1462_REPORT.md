# Android Service control-listener report (ADR 1462)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`
- Physical device execution: not run

## Implemented evidence

- JNI creates a bounded native listener on an ephemeral all-interface port,
  accepts without blocking while idle, retains a successful catalog link and
  reconciles it before Runtime destruction.
- `AudioNodeService` owns the listener and background accept worker and clears
  the temporary Java credential bytes.
- The shared Android snapshot reports `connected`, `active` and connection
  count one only after the native initial exchange succeeds.

## Verification

```text
cargo fmt --all -- --check
cargo test -p capyio-android-jni --tests
cargo clippy -p capyio-android-jni --all-targets -- -D warnings
platform/android/gradlew.bat capyioCheck
```

All checks passed. Android `capyioCheck` cross-compiled and packaged the new JNI
symbols and Java owner, ran lint and 274 shared-composition assertions, and
completed 91 tasks (24 executed, 67 up-to-date).

This does not prove a physical peer connection or discovery advertisement. An
ordinary user still lacks enrollment and automatic endpoint discovery.
