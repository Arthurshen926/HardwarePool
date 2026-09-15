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

The protocol crate now supplies the preceding decode step. Its dedicated
CatalogSnapshot decoder rejects a missing Session ID, a different control
payload or an absent/invalid Node descriptor before returning typed message,
Session and Core Node identities. A future authenticated control host can call
the decoder and Runtime admission boundary without reimplementing wire
validation.

The symmetric encoder accepts a typed Session ID and validated Core Node
catalog, assigns a fresh message ID, converts the complete public descriptor
and enforces the common 1 MiB control-envelope limit. Platform hosts therefore
do not need to construct CatalogSnapshot envelopes independently.

`capyio-control-host` now composes both steps. A caller supplies one exact
Session/peer binding after connection admission; the host decodes a catalog,
checks both identities, suppresses recently applied duplicate message IDs and
then invokes Runtime. Only successful Runtime application consumes an ID, and
retention is fixed at 256.

The Windows Tauri host now consumes `DemoLab` only to create deterministic
bootstrap state. Its long-lived `DesktopNodeState` owns the `NodeRuntime`,
bootstrap Session ID and demo Route allow-list; every existing desktop feature
worker and UI command shares that Runtime directly. This is the required state
ownership seam for a later authenticated control connection, not a network or
authentication implementation.

## Evidence

- `cargo test -p capyio-testkit`: 18 passed.
- `cargo test -p capyio-protocol`: 11 passed.
- `cargo test -p capyio-control-host`: 6 passed.
- `cargo test -p capyio-desktop --lib`: 55 passed, 6 intentionally ignored
  physical/installed-device tests.
- `cargo clippy -p capyio-runtime -p capyio-testkit --all-targets -- -D warnings`:
  passed.
- Three focused regressions cover accepted replacement and Route
  reconciliation, wrong-identity transactional rejection and Suspended-Session
  transactional rejection.
- `cargo xtask ci`: passed, including workspace format/check/Clippy/tests,
  documentation and manifest validation, Adapter smoke, desktop typecheck and
  production build.

## Non-claims and remaining work

- The Runtime method receives an already decoded, host-supplied Core descriptor.
  It opens no socket and does not authenticate a peer. The adjacent protocol
  helper decodes Protobuf but likewise grants no trust.
- No Android or Windows host currently invokes it from a network control
  connection. Windows now has the necessary process-owned Runtime seam only.
- Capability publication, Route authorization, encrypted data-plane binding,
  replay defense, reconnect and pairing remain separate work.
- No APK, driver, service, Display branch or system configuration was changed.

## Internal dependency note

- Purpose: keep protocol decoding and Runtime mutation composition identical
  across Android and desktop hosts.
- Dependencies: workspace `capyio-core`, `capyio-protocol`, `capyio-runtime`
  and `thiserror`; no new third-party package or network stack was added.
- License/maintenance: Apache-2.0 workspace crate maintained with the main
  Runtime and protocol contracts.
- Alternatives considered: duplicating admission in each platform host would
  create divergent identity/replay behavior; placing it in Core or Runtime
  would invert the established Protocol/Runtime dependency boundary.
