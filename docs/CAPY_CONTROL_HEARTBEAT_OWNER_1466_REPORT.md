# Platform control-heartbeat owner report (ADR 1466)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`

Windows Desktop now transfers an authenticated, catalog-exchanged link to a
background controller. The controller initiates a Session-bound heartbeat,
validates the exact acknowledgement and owns close/reconciliation independently
of Tauri UI snapshots. Android's retained accepted link receives and echoes the
same heartbeat from its Service worker, then returns to listener mode after a
bounded failure.

Verification commands:

```text
cargo test -p capyio-control-link -p capyio-android-jni
cargo check -p capyio-desktop
platform/android/gradlew.bat -p platform/android capyioCheck
cargo xtask validate-docs
```

The real loopback mTLS request/acknowledgement test is owned by
`capyio-control-link`. Android `capyioCheck` cross-compiles and packages the JNI
and Java Service composition. Physical loss/recovery timing is not claimed
without a reachable authorized device.
