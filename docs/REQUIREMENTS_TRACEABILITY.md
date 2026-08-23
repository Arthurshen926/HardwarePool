# CapyIO Requirements Traceability

> Updated: 2026-08-24 for `CAPY-FOUNDATION-002`.
>
> Scope: the 84 normative Requirement IDs in `PRODUCT_REQUIREMENTS.md`.

## Status rules

- `verified`: executable evidence or an offline repository rule checks the
  requirement at its current scope.
- `implemented`: implementation exists, but the required hosted/platform
  evidence is not retained yet.
- `planned`: the complete requirement belongs to a future Gate. Foundation
  types or documentation do not upgrade a future product requirement.

Paths and test names below are evidence locators, not claims that future
hardware, networking, security or performance behavior has run.

## Gate 0–3 foundation acceptance evidence

| Acceptance ID | Evidence |
|---|---|
| `G0-3-01` | CapyIO naming is checked across Cargo, `capyio.v1` Protobuf and UI by `scripts/validate_repository.py`; Gate 1 results are retained in `GATE_0_3_REPORT.md`. |
| `G0-3-02` | `crates/capyio-core` models Node, AdapterInstance, Capability, Port, Route, Session and Problem; Core tests and repository dependency validation cover the boundary. |
| `G0-3-03` | `capyio-core` Route tests cover Source/Sink direction, Profile compatibility and rejection paths. |
| `G0-3-04` | `capyio-testkit` and Runtime tests operate four deterministic cross-direction Routes and prove independent stop/failure behavior. |
| `G0-3-05` | `capyio-protocol` round-trip tests and `cargo xtask validate-manifests` validate the current protocol and manifest contracts. |
| `G0-3-06` | `cargo xtask adapter-smoke` exercises initialize, probe, catalog, health, prepare, start, status, stop, shutdown and scoped child failure. |
| `G0-3-07` | Browser Mock and Tauri Mock share the UI DTO contract; frozen frontend typecheck/build validate Quick Actions and Workspace. |
| `G0-3-08` | `cargo xtask ci` is the local gate; hosted workflows run repository, Rust, Adapter and UI checks on the exact PR head. Current hosted results remain pending until pushed. |
| `G0-3-09` | `PRODUCT_REQUIREMENTS.md`, `BUILD_STATUS.md` and the UI explicitly exclude real hardware, APK, driver and production-network claims. |

## Normative requirement matrix

| Requirement ID | Status | Target Gate | Evidence or planned proof |
|---|---|---|---|
| `FR-SCEN-001` | planned | Gate 8 | MicYou phone microphone to desktop/system-microphone evidence. |
| `FR-SCEN-002` | planned | Gate 7 | Audio Share remote-speaker playback and lifecycle evidence. |
| `FR-SCEN-003` | planned | Gate 6 | Gamepad input plus independent reverse-haptics Routes. |
| `FR-SCEN-004` | planned | Gate 9 | Camera preview and Windows virtual-camera evidence. |
| `FR-SCEN-005` | planned | Gates 10–11 | Mirror evidence at Gate 10 and separate extended-display evidence at Gate 11. |
| `FR-SCEN-006` | planned | Gate 10 | Platform keyboard, pointer and touchpad projection tests. |
| `FR-SCEN-007` | planned | Gate 12 | Multi-stream provenance, visualization and recording evidence. |
| `FR-SCEN-008` | planned | Gate 13 | Multi-node temporary-workspace composition evidence. |
| `FR-SCEN-009` | planned | Gate 5 | Recorded IMU fixture, replay, gap and abnormal-path tests. |
| `FR-NODE-001` | verified | Gate 2 | Typed Node descriptor and protocol round-trip tests in `capyio-core` and `capyio-protocol`. |
| `FR-NODE-002` | verified | Gate 2 | Core has no global NodeRole; deterministic fixtures own mixed-direction Ports. |
| `FR-NODE-003` | verified | Gate 2 | `capyio-testkit` catalogs contain Source and Sink Ports on both Nodes. |
| `FR-NODE-004` | verified | Gate 2 | Symmetric catalog DTO/round-trip and deterministic two-Node fixture tests. |
| `FR-NODE-005` | planned | Gate 14 | Pairing and explicit authorization tests for unknown/offline Nodes. |
| `FR-SESSION-001` | verified | Gate 2 | Typed Session model and protocol session round-trip coverage. |
| `FR-SESSION-002` | planned | Gate 14 | Expiring, independently revocable Capability/Route authorization. |
| `FR-SESSION-003` | planned | Gate 14 | Authenticated reconnect and stale-epoch replay rejection. |
| `FR-ADAPTER-001` | verified | Gate 2 | Catalog validation and Core tests enforce Adapter ownership for Capabilities. |
| `FR-ADAPTER-002` | verified | Gate 3 | Manifest model/schema tests cover InProcess, Sidecar, ExternalService and DriverBacked declarations. |
| `FR-ADAPTER-003` | verified | Gate 2 | Adapter descriptors, Runtime snapshots and UI expose state, health and owned Capabilities/Routes. |
| `FR-ADAPTER-004` | verified | Gate 3 | Adapter Host crash-isolation test scopes failure to owned Capabilities/Routes. |
| `FR-ADAPTER-005` | verified | Gate 3 | Core backend/interoperability validation and `ADAPTER_MODEL.md` constrain AdapterManaged Routes. |
| `FR-ADAPTER-006` | verified | Gate 3 | Committed manifest schema, Rust validation tests and `cargo xtask validate-manifests`. |
| `FR-CAP-001` | verified | Gate 2 | Capability descriptor validation covers class, owner, availability, permission and metadata. |
| `FR-CAP-002` | verified | Gate 2 | Catalog validation rejects routable Capabilities without valid Ports. |
| `FR-CAP-003` | verified | Gate 2 | Capability has no direction field; PortDirection is independently tested. |
| `FR-CAP-004` | verified | Gate 2 | Core permits mixed-direction Ports on one Capability and fixtures exercise compound catalogs. |
| `FR-CAP-005` | verified | Gate 2 | Custom Capability class remains explicit through Core/protocol conversion. |
| `FR-CAP-006` | verified | Gate 2 | Panels, projections and exports use Adapter-owned Capability/Port descriptors in fixtures and UI. |
| `FR-PORT-001` | verified | Gate 2 | PortDirection enum and Route direction-rejection tests. |
| `FR-PORT-002` | verified | Gate 2 | Port descriptor validation and protocol round trips cover Profile, formats, QoS, clock, availability and permission. |
| `FR-PORT-003` | verified | Gate 2 | ProfileId is transport/platform-neutral and Core dependency validation guards the boundary. |
| `FR-PORT-004` | planned | Gate 5 | Standard IMU envelope must preserve timestamps, sequence, units, frames, accuracy and calibration. |
| `FR-PORT-005` | verified | Gate 2 | Profile-major compatibility and unknown/zero protocol enum rejection tests. |
| `FR-ROUTE-001` | verified | Gate 2 | Route constructor requires exactly one Source PortRef and one Sink PortRef. |
| `FR-ROUTE-002` | verified | Gate 2 | Direction, Profile, format, QoS and interoperability rejection tests. |
| `FR-ROUTE-003` | verified | Gate 2 | Route state enum and lifecycle transition tests cover all eight states. |
| `FR-ROUTE-004` | verified | Gate 2 | Invalid transitions return typed Core errors in Route tests. |
| `FR-ROUTE-005` | verified | Gate 2 | Runtime multi-Route tests prove independent stop/failure behavior. |
| `FR-ROUTE-006` | verified | Gate 2 | Deterministic fixture activates opposite-direction Routes simultaneously. |
| `FR-ROUTE-007` | verified | Gate 2 | RouteBackend enum is explicit; backend support/interoperability rejection is tested. |
| `FR-ROUTE-008` | verified | Gate 2 | Route descriptor/snapshot round trips retain format, QoS, authorization, diagnostics and epoch. |
| `FR-DIAG-001` | verified | Gate 2 | Problem descriptor validation and protocol round-trip tests cover stable structured fields. |
| `FR-DIAG-002` | verified | Gate 2 | Runtime tests assert bounded, monotonically sequenced event retention. |
| `FR-DIAG-003` | verified | Gate 3 | Adapter Host tests exercise bounded/truncated stderr retention. |
| `FR-DIAG-004` | verified | Gate 3 | Sidecar stdout/stderr separation tests and repository secret scanning; finite sample data stays test-only. |
| `FR-PROTO-001` | verified | Gate 2 | `capyio.v1` Envelope constants and binary round-trip/version tests. |
| `FR-PROTO-002` | verified | Gate 2 | Reserved Protobuf ranges plus offline duplicate/reserved-field validation. |
| `FR-PROTO-003` | planned | Gate 14 | Production control additions still need heartbeat and complete authorization semantics. |
| `FR-PROTO-004` | verified | Gate 3 | Protocol/Sidecar contracts exclude continuous payloads; generic Route-control round-trip tests enforce the seam. |
| `FR-PROTO-005` | verified | Gate 2 | Explicit validated conversions between generated Protobuf and Core descriptors. |
| `FR-PROTO-006` | verified | Gate 2 | Unknown/zero enum and unsupported-major tests in `capyio-protocol`. |
| `FR-PROTO-007` | verified | Gate 3 | NDJSON codec and Sidecar smoke verify stdin/stdout control with stderr-only logs. |
| `FR-UX-001` | verified | Gate 2 | Quick Actions present task-oriented Route cards without Adapter/Port details. |
| `FR-UX-002` | planned | Gate 4 | Versioned Route Templates and permission/result status UX. |
| `FR-UX-003` | planned | Gate 4 | Complete Workspace navigation for all required object families. |
| `FR-UX-004` | planned | Gate 4 | Accessible list/card Route Builder implementation and tests. |
| `FR-UX-005` | verified | Gate 2 | Browser/Tauri Mock share TypeScript DTOs and visibly label simulated mode/metrics. |
| `FR-UX-006` | verified | Gate 2 | Per-Route UI actions and Runtime tests prove independent toggles. |
| `FR-UX-007` | verified | Gate 2 | Built-in preview/Panel representation only; repository contains no dynamic Panel market. |
| `FR-PLAT-001` | verified | Gate 3 | Desktop Sidecar deployment boundary and hosted Adapter-smoke workflow on Windows/Linux/macOS. |
| `FR-PLAT-002` | planned | Gate 5 | First mobile in-process/platform-managed Adapter lifecycle evidence. |
| `FR-PLAT-003` | planned | Gate 4 | Headless Runtime/service lifecycle separated from UI/window closure. |
| `FR-PLAT-004` | planned | Gate 4 | Product fallback selection and observable Projection support levels. |
| `FR-PLAT-005` | verified | Gate 1 | PRD, platform support and UI disclaim unsupported Android global virtual devices. |
| `FR-PLAT-006` | planned | Gate 8 | Android microphone permission, indicator and foreground lifecycle tests. |
| `NFR-SEC-001` | planned | Gate 14 | Mutual authentication, encryption, replay defense and downgrade binding. |
| `NFR-SEC-002` | planned | Gate 14 | Time-bound per-Capability/Route grant and immediate revoke tests. |
| `NFR-SEC-003` | verified | Gate 1 | Architecture/driver boundary docs and offline Core/driver dependency rules. |
| `NFR-SEC-004` | verified | Gate 2 | Narrow Tauri command surface/CSP and repository rule excluding shell/updater permissions. |
| `NFR-SEC-005` | verified | Gate 1 | PRD, Security Model, Build Status and UI explicitly label the foundation insecure/mock. |
| `NFR-STAB-001` | planned | Gate 8 | Real system-audio endpoint disconnect/restart evidence in an approved target. |
| `NFR-STAB-002` | verified | Gate 3 | Bounded Runtime events, RPC messages/correlations, line readers and stderr retention tests. |
| `NFR-STAB-003` | verified | Gate 3 | Runtime catalog replacement tests invalidate dependent Route epochs and exercise recovery. |
| `NFR-STAB-004` | verified | Gate 3 | Scoped Adapter failure and unrelated Route/catalog update tests. |
| `NFR-STAB-005` | verified | Gate 1 | Testing/evidence rules and Build Status prohibit unrun soak/hardware claims. |
| `NFR-RT-001` | planned | Gate 7 | First real-time audio callback audit and stress evidence. |
| `NFR-RT-002` | planned | Gate 7 | Fixed-capacity callback-path data structures and overflow tests. |
| `NFR-RT-003` | planned | Gate 7 | Clock-domain timestamps and user-mode recovery/resampling evidence. |
| `NFR-MAINT-001` | implemented | Gate 3 | Exact-head Windows/Linux/macOS Rust and Adapter CI is configured; current hosted run is pending. |
| `NFR-MAINT-002` | verified | Gate 3 | Cargo dependency boundaries, unit tests and `scripts/validate_repository.py`. |
| `NFR-MAINT-003` | verified | Gate 3 | ADRs, compatibility documentation and protocol/Core tests cover foundation public changes. |
| `NFR-MAINT-004` | planned | Gate 5 | First SensorServer integration provenance, license, imported-path and risk record. |
| `NFR-MAINT-005` | verified | Gate 1 | `xtask` commands are non-privileged; AGENTS and offline rules exclude deployment operations. |

## Updating this report

Any PRD ID addition, removal or spelling change must update this table in the
same change. `cargo xtask validate-docs` rejects malformed/duplicate PRD IDs,
missing/unknown rows, invalid statuses or Gates, empty evidence, future rows not
marked `planned`, and incomplete Gate 0–3 acceptance evidence.
