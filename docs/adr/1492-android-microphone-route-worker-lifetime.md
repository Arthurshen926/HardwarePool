# ADR 1492: Android microphone Route-bound worker lifetime

Status: accepted for the internal adapter handoff only

Date: 2026-09-28

## Decision

Add a one-shot native audio Route guard to the Android sender. A guard binds
the exact Session/Route/Stream/epoch and finite deadline from ADR 1491. It uses
both wall-clock expiry and the original monotonic duration budget: moving the
wall clock backwards cannot extend that budget. Revocation and observed expiry
are terminal; an expired guard cannot be revived by a clock correction.

The sender checks before queue polling and again before packet submission.
An idle queue therefore does not prevent expiry handling. Guard failure reports
`CAPY.ANDROID.LAN_ROUTE_ENDED` and closes the socket. Explicit Stop revokes the
guard and joins the worker with the existing bound. A packet already inside a
socket operation is not claimed to be retractable on concurrent revocation.

The internal MicrophoneSourceAdapter configureRoute entry accepts only matching
live bindings while stopped. It supplies the guard to the actual sender, checks
before capture startup, and revokes on Stop or capture exit. Guard failure uses
the existing generation-scoped media-failure path to stop AudioRecord. Route
configuration is ephemeral, not written into trusted-lab peer preferences.

## Integration and safety boundary

The Service/control negotiation path does not call configureRoute yet. No UI
entry, platform permission, foreground-service declaration, wire schema or
dependency changes. Existing explicitly configured trusted-lab audio still uses
the compatibility constructor without a guard. This must not be called a
completed authenticated audio Route or encrypted transport.

The future control owner must establish consent, live Session/Route/epoch,
format and leases before creating the guard, and revoke it immediately on
disconnect, lease loss or authorization revocation. The guard is not a token
that independently proves any of those facts. The guard check runs on the
sender worker, never inside an Android realtime callback.

## Verification

Dependency-free tests cover exact expiry, monotonic budget after wall-clock
rollback, sticky revocation, queued audio rejection, idle-worker expiry,
socket closure, one-shot restart rejection and a live PCM loopback followed by
Stop. Android Java compilation checks the platform adapter composition.
No APK was installed and no physical microphone regression is claimed here.

Recorded checks on 2026-09-28:

- `:node-contract:nativeLanContractTest`: 175 assertions passed.
- `:app:compileDebugJavaWithJavac -x :app:buildRustJni -x :app:buildSharedUi`:
  passed after retry with approved SDK access. The restricted attempt failed
  with AccessDeniedException reading SDK build-tools/36.0.0/core-lambda-stubs.jar.
  This is Java compilation only, not a fresh JNI/UI build or packaged APK.
- `cargo test --offline -p capyio-native-audio-lan`: 22 tests passed.
- `cargo clippy --offline -p capyio-native-audio-lan --all-targets -- -D warnings`:
  passed.
- `cargo xtask validate-docs` and `git diff --check`: passed.
