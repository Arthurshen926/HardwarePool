# Android accepted catalog exchange report (ADR 1460)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`
- Physical device execution: not required for this hardware-free loopback slice

## Evidence

`accepted_authenticated_peer_exchanges_catalogs_and_remains_owned` creates real
loopback TLS 1.3 server/client identities, exact peer leaf pins and a fresh
Node-bound Session. Android accepts the channel, applies the Windows catalog,
sends its selected local catalog and retains a live link. Both Runtime snapshots
contain the authenticated peer and its published capabilities.

`invalid_first_catalog_closes_the_admitted_android_session` sends a validly
framed catalog with the wrong Session ID over the authenticated channel. Android
rejects it and the provisional Session is observed closed.

## Verification

```text
cargo fmt --all -- --check
cargo test -p capyio-android-jni --tests
cargo clippy -p capyio-android-jni --all-targets -- -D warnings
```

All checks passed. This evidence does not include Android socket ownership,
Keystore-to-listener wiring, discovery advertisement, reconnect, ongoing
control messages, leases, Routes or a physical device.
