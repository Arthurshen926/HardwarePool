# Android control-listener report (ADR 1461)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`
- Physical device execution: not run; JNI lifecycle is not part of this slice

## Evidence

The loopback test constructs a canonical single-peer directory and an encoded
typed server credential, binds `127.0.0.1:0`, recovers the actual port, performs
mutual TLS 1.3 with exact leaf pinning and fresh Node/Session binding, then
completes bidirectional catalog exchange into Android Runtime. Separate tests
prove zero and multiple active peers fail before credential decoding and that a
wrong-Session first catalog closes the admitted Session.

## Verification

```text
cargo fmt --all -- --check
cargo test -p capyio-android-jni --tests
cargo clippy -p capyio-android-jni --all-targets -- -D warnings
platform/android/gradlew.bat capyioCheck
```

All checks passed. The full Android gate cross-compiled the listener and TLS
stack for ARM64, packaged the JNI library into the unified debug APK, ran lint
and all hardware-free contracts (91 tasks; 17 executed, 74 up-to-date). This
report must not be read as evidence that the Java Service starts the listener,
advertises a port, accepts a physical Windows peer or reconnects.
