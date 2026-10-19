# Android active-credential admission report (ADR 1459)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`
- Physical device execution: not run; current endpoint was unavailable

## Implemented evidence

- The shared pairing directory now provides an active-only iterator while
  preserving revoked records in the full directory.
- The bounded Android JNI projection returns credential handles for active
  records only.
- `AudioNodeService` resolves every projected handle through Android Keystore,
  validates the recovered typed control bundle, and clears its temporary byte
  array before declaring pairing storage ready.
- A missing, corrupt, unsupported or undecryptable active credential fails the
  pairing host state closed without disabling local hardware inventory.

## Verification

```text
cargo fmt --all -- --check
cargo test -p capyio-pairing -p capyio-android-jni --tests
cargo clippy -p capyio-pairing -p capyio-android-jni --all-targets -- -D warnings
platform/android/gradlew.bat capyioCheck
```

Results: all Rust tests passed; Clippy passed; the ARM64 JNI library, shared UI,
unified debug APK and Android test APK built; Android lint and all hardware-free
contract tests passed. `capyioCheck` reported 91 tasks, 33 executed and 58
up-to-date. The first sandboxed build attempt failed because Windows denied NDK
`clang`; the same offline command succeeded outside the sandbox.

This does not prove physical Android Keystore recovery for a populated paired
directory, peer TLS, discovery, catalog exchange, reconnect or Route control.
