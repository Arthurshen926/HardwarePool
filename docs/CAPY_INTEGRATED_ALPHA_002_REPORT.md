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
Both canonical encoding and inbound validation now enforce semantic catalog
bounds inside that byte ceiling: 64 Adapters, 256 Capabilities, 32 Ports per
Capability, 32 formats per Port, 64 metadata/format-parameter entries, 16
protocol versions and 2 KiB per catalog string. Excess counts and strings have
typed protocol errors and are rejected before Core conversion.

`capyio-control-host` now composes both steps. A caller supplies one exact
Session/peer binding after connection admission; the host decodes a catalog,
checks both identities, suppresses recently applied duplicate message IDs and
then invokes Runtime. Only successful Runtime application consumes an ID, and
retention is fixed at 256.

For outbound publication, the same host boundary projects an already validated
local catalog from a set of selected Source Capability IDs. Unselected
Source-bearing Capabilities are removed, Sink/Control-only Capabilities remain
visible, Adapter ownership is pruned to the retained set, and the result is
revalidated before Session-bound encoding. Unknown, non-Source and Offline
selections fail closed.
The bound outbound method derives its Session ID from the same admitted
Session/peer tuple used by inbound catalog admission; platform callers cannot
supply a second, potentially mismatched Session ID on that path.

The Windows Tauri host now consumes `DemoLab` only to create deterministic
bootstrap state. Its long-lived `DesktopNodeState` owns the `NodeRuntime`,
bootstrap Session ID and demo Route allow-list; every existing desktop feature
worker and UI command shares that Runtime directly. This is the required state
ownership seam for a later authenticated control connection, not a network or
authentication implementation.
Windows desktop publication selection now calls the shared projection on a
cloned candidate set before committing it. This removes duplicated eligibility
policy and permits an already-selected Source to be deselected after it becomes
Offline.

The Android Service-owned selection now follows the same publication policy at
its Java boundary. A pure Android-local projector revalidates every selected
Source against the current catalog, removes unselected Source-bearing
Capabilities and retains Sink/Control-only inventory. Selection commits only
after projection succeeds, while an already-selected Source may still be
removed after it becomes Offline. This prepares an Android catalog for a future
authenticated encoder invocation; it does not send the catalog.

ADR 1404 supplies the stable identity layer needed by the next Android Core
projection. Both Java and Rust derive Adapter, Capability and Port UUIDv5 values
from the persisted Node UUID and typed canonical-key paths, with shared fixed
vectors. That identity layer does not itself fill Core Adapter, QoS or
interoperability fields; ADR 1405 supplies the separate projection.

ADR 1405 adds the complete pure host projection behind that identity layer. A
closed Android DTO explicitly supplies Adapter attachment/type/version and
Route modes plus Port Profile/formats/QoS/clock/interoperability. The projector
builds a Core-valid ownership graph; a hardware-free integration test selects
the microphone Source, retains the speaker Sink and round-trips the result
through the existing Session-bound Protobuf codec. The Java Service now exposes
the selected local publication as matching schema-v1 JSON; Rust bounds it to
64 KiB and rejects unknown fields and versions before projection. This closes
the local handoff contract but not native Runtime invocation or delivery.

## Evidence

- `cargo test -p capyio-testkit`: 18 passed.
- `cargo test -p capyio-protocol`: 13 integration tests and 3 unit tests passed.
- `cargo test -p capyio-control-host`: 9 passed.
- `cargo test -p capyio-desktop --lib`: 56 passed, 6 intentionally ignored
  physical/installed-device tests.
- `platform/android/gradlew.bat --offline :app:capabilityCompositionTest`:
  passed, 219 assertions.
- `platform/android/gradlew.bat --offline :app:verifyDebugEntrypoints`: passed;
  the unified Android service and bridge compiled against the existing SDK.
- `cargo test -p capyio-android-host --offline`: 16 tests passed, including
  three cross-language identity tests and three Core publication tests.
- `platform/android/gradlew.bat --offline
  :node-contract:capabilityCatalogContractTest`: passed, 22 assertions.
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
  connection. Windows now has the necessary process-owned Runtime seam, and
  Android can produce a locally filtered, Core-shaped JSON catalog only.
- Platform invocation/delivery of projected catalogs, Route authorization,
  encrypted data-plane binding, replay defense, reconnect and pairing remain
  separate work.
- No APK, driver, service, Display branch or system configuration was changed.

## Internal dependency note

- Purpose: keep protocol decoding and Runtime mutation composition identical
  across Android and desktop hosts.
- Dependencies: workspace `capyio-core`, `capyio-protocol`, `capyio-runtime`
  and `thiserror`; Windows desktop now consumes the internal control-host crate.
  ADR 1404 adds the already locked RustCrypto `sha1` 0.11 crate to the Android
  host solely for UUIDv5 fixed-vector parity. ADR 1405 uses existing workspace
  `serde`/`serde_json` for the closed local handoff. No network stack was added.
- License/maintenance: Apache-2.0 workspace crate maintained with the main
  Runtime and protocol contracts.
- Alternatives considered: duplicating admission in each platform host would
  create divergent identity/replay behavior; placing it in Core or Runtime
  would invert the established Protocol/Runtime dependency boundary.
