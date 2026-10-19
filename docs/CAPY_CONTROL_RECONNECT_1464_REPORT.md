# Authenticated control reconnect report (ADR 1464)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`

A real loopback mTLS test admits both Nodes, closes and reconciles both exact
Sessions, negotiates another fresh Session over new TLS channels and verifies
one ready Session remains after reconnect. Existing tests continue to reject a
second live Session and wrong bindings.

Verification passed:

- `cargo test -p capyio-control-link -p capyio-android-jni --tests`;
- Clippy for both crates with warnings denied;
- desktop library: 80 passed, 10 physical tests ignored;
- desktop Clippy and shared UI typecheck.

This does not detect an idle half-open socket or automatically retry after
network loss.
