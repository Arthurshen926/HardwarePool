# Live publication-catalog synchronization report (ADR 1467)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`

Windows and Android now send later Local sharing changes over their retained
authenticated control link. Windows queues a nonblocking latest-publication
signal; Android marks its publication dirty during native Runtime replacement
and sends it from the socket-owning Service worker. Both sides apply received
catalogs under the exact established Session/peer binding.

Verification commands:

```text
cargo test -p capyio-control-link -p capyio-android-jni
cargo test -p capyio-desktop
cargo clippy -p capyio-control-link -p capyio-android-jni -p capyio-desktop --all-targets -- -D warnings
platform/android/gradlew.bat -p platform/android capyioCheck
cargo xtask validate-docs
```

The real loopback mTLS test proves that a catalog update can retain wire order,
be dispatched and applied before the pending heartbeat acknowledgement. The
Android gate packages the JNI/Service dirty-publication path. No physical
tablet-to-Windows toggle/inventory result is claimed in this report.
