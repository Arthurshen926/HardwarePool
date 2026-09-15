# CAPY-INTEGRATED-ALPHA-002 report

Date: 2026-09-16

Branch: `codex/capyio-integrated-alpha`

## Result

`NodeRuntime` now accepts a complete peer Capability catalog through an
existing Ready Session. The catalog Node ID must exactly match that Session's
remote Node, and transport-observed online state remains locally owned rather
than being overwritten by peer-supplied catalog data.

Acceptance is transactional: the Runtime clones its bounded state, validates
and reconciles the candidate, and commits only after every operation succeeds.
Wrong identity, unknown Session, suspended Session, offline peer or invalid
catalog therefore leaves peers, Sessions, Routes, Problems and events
unchanged.

An accepted replacement revalidates every Route that references the peer.
Removed or incompatible endpoints move only dependent Routes Offline with the
existing structured catalog Problem and fresh epoch behavior. Unrelated and
opposite-direction Routes remain unchanged. A bounded
`peer_catalog_accepted` Runtime event records the successful admission.

## Evidence

- `cargo test -p capyio-testkit`: 18 passed.
- `cargo clippy -p capyio-runtime -p capyio-testkit --all-targets -- -D warnings`:
  passed.
- Three focused regressions cover accepted replacement and Route
  reconciliation, wrong-identity transactional rejection and Suspended-Session
  transactional rejection.
- `cargo xtask ci`: passed, including workspace format/check/Clippy/tests,
  documentation and manifest validation, Adapter smoke, desktop typecheck and
  production build.

## Non-claims and remaining work

- This method receives an already decoded, host-supplied Core descriptor. It
  opens no socket and does not authenticate a peer or decode Protobuf.
- No Android or Windows host currently invokes it from a network control
  connection.
- Capability publication, Route authorization, encrypted data-plane binding,
  replay defense, reconnect and pairing remain separate work.
- No APK, driver, service, Display branch or system configuration was changed.
