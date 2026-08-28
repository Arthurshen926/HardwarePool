# CapyIO Requirements Traceability

> Updated: 2026-08-28 for the completed Speaker functional Gate.
>
> Scope: the 84 normative Requirement IDs in `PRODUCT_REQUIREMENTS.md`.

## Status rules

- `verified`: executable evidence or an offline repository rule checks the
  requirement at its current scope.
- `implemented`: implementation exists, but the required hosted/platform
  evidence is not retained yet.
- `planned`: the complete requirement belongs to a future Gate. Foundation
  types or documentation do not upgrade a future product requirement.

An active Gate may contain verified small-slice evidence while the Gate itself
remains open. Only the behavior named in that row is claimed; later live
hardware, lifecycle or production evidence remains planned separately.

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
| `G0-3-08` | `cargo xtask ci` is the local gate; PR #10 passed repository, UI, Windows/Linux/macOS Rust/Adapter and Windows Tauri exact-head checks before merge commit `5f5b81f`. |
| `G0-3-09` | `PRODUCT_REQUIREMENTS.md`, `BUILD_STATUS.md` and the UI explicitly exclude real hardware, APK, driver and production-network claims. |

## Normative requirement matrix

| Requirement ID | Status | Target Gate | Evidence or planned proof |
|---|---|---|---|
| `FR-SCEN-001` | planned | Gate 8 | MicYou phone microphone to desktop/system-microphone evidence. |
| `FR-SCEN-002` | implemented | Gate 7 | Controlled-lab evidence: Gate 7A retains system-mix playback; Gate 7B proves a separately enumerated `CapyIO Speaker`, real post-mix PCM, Android playback, endpoint volume/mute and service-owned ordinary-user control. Release qualification, background/focus/performance and production security continue in `CAPY-AUDIO-001C`. |
| `FR-SCEN-003` | planned | Gate 6 | Gamepad input plus independent reverse-haptics Routes. |
| `FR-SCEN-004` | planned | Gate 9 | Camera preview and Windows virtual-camera evidence. |
| `FR-SCEN-005` | planned | Gates 10–11 | Mirror evidence at Gate 10 and separate extended-display evidence at Gate 11. |
| `FR-SCEN-006` | planned | Gate 10 | Platform keyboard, pointer and touchpad projection tests. |
| `FR-SCEN-007` | planned | Gate 12 | Multi-stream provenance, visualization and recording evidence. |
| `FR-SCEN-008` | planned | Gate 13 | Multi-node temporary-workspace composition evidence. |
| `FR-SCEN-009` | verified | Gate 5 | Bounded recorded-style IMU fixture replay covers independent sinks, gaps, overflow, stale epochs and abnormal paths. |
| `FR-NODE-001` | verified | Gate 2 | Typed Node descriptor and protocol round-trip tests in `capyio-core` and `capyio-protocol`. |
| `FR-NODE-002` | verified | Gate 2 | Core has no global NodeRole; deterministic fixtures own mixed-direction Ports. |
| `FR-NODE-003` | verified | Gate 2 | `capyio-testkit` catalogs contain Source and Sink Ports on both Nodes. |
| `FR-NODE-004` | verified | Gate 2 | Symmetric catalog DTO/round-trip and deterministic two-Node fixture tests. |
| `FR-NODE-005` | planned | Gate 14 | Pairing and explicit authorization tests for unknown/offline Nodes. |
| `FR-SESSION-001` | verified | Gate 2 | Typed Session model and protocol session round-trip coverage. |
| `FR-SESSION-002` | planned | Gate 14 | Expiring, independently revocable Capability/Route authorization. |
| `FR-SESSION-003` | planned | Gate 14 | B3B proves explicit local recovery advances the Route epoch; authenticated reconnect and wire-level stale-epoch replay rejection remain Gate 14. |
| `FR-ADAPTER-001` | verified | Gate 2 | Catalog validation and Core tests enforce Adapter ownership for Capabilities. |
| `FR-ADAPTER-002` | verified | Gate 3 | Manifest model/schema tests cover InProcess, Sidecar, ExternalService and DriverBacked declarations. |
| `FR-ADAPTER-003` | verified | Gate 2 | Adapter descriptors, Runtime snapshots and UI expose state, health and owned Capabilities/Routes. |
| `FR-ADAPTER-004` | verified | Gate 3 | Adapter Host crash-isolation test scopes failure to owned Capabilities/Routes. |
| `FR-ADAPTER-005` | verified | Gate 3 | Core backend/interoperability validation constrains AdapterManaged Routes; the SensorServer external-protocol boundary has separate mapping tests and provenance. |
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
| `FR-PORT-004` | verified | Gate 5 | IMU fixture, SensorServer pairing tests and the physical Tauri panel preserve envelope plus per-component timestamps, sequence, units, frame, accuracy and calibration. |
| `FR-PORT-005` | verified | Gate 2 | Profile-major compatibility and unknown/zero protocol enum rejection tests. |
| `FR-ROUTE-001` | verified | Gate 2 | Route constructor requires exactly one Source PortRef and one Sink PortRef. |
| `FR-ROUTE-002` | verified | Gate 2 | Direction, Profile, format, QoS and interoperability rejection tests. |
| `FR-ROUTE-003` | verified | Gate 2 | Route tests cover all eight states; B3B Adapter completions exercise staged Starting/Active/Offline/recovery/Stopping/Stopped in Runtime. |
| `FR-ROUTE-004` | verified | Gate 2 | Invalid transitions return typed Core errors; B3B platform callbacks mutate lifecycle only through Runtime commands. |
| `FR-ROUTE-005` | verified | Gate 2 | Runtime multi-Route tests prove independent stop/failure behavior. |
| `FR-ROUTE-006` | verified | Gate 2 | Deterministic fixture activates opposite-direction Routes simultaneously. |
| `FR-ROUTE-007` | verified | Gate 2 | RouteBackend enum is explicit; backend support/interoperability rejection is tested. |
| `FR-ROUTE-008` | verified | Gate 2 | Route descriptor/snapshot round trips retain format, QoS, authorization, diagnostics and epoch. |
| `FR-DIAG-001` | verified | Gate 2 | Problem validation/protocol tests plus B3B retain stable Route-related SensorServer disconnect diagnostics. |
| `FR-DIAG-002` | verified | Gate 2 | Runtime tests assert bounded monotonic events; B3B projects Route state, epoch and Problem code to the Tauri DTO. |
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
| `NFR-SEC-004` | verified | Gate 2 | Narrow Tauri command surface/CSP and repository rule exclude shell/updater and arbitrary networking; the physical lab accepts only a typed IP literal and port. |
| `NFR-SEC-005` | verified | Gate 1 | PRD, Security Model, Build Status and UI explicitly label the foundation insecure/mock. |
| `NFR-STAB-001` | planned | Gate 8 | Real system-audio endpoint disconnect/restart evidence in an approved target. |
| `NFR-STAB-002` | verified | Gate 3 | Bounded Runtime events, RPC messages/correlations, line readers and stderr retention tests. |
| `NFR-STAB-003` | verified | Gate 3 | Catalog tests and B3B disconnect/retry tests invalidate failed epochs and require explicit recovery with a later epoch. |
| `NFR-STAB-004` | verified | Gate 3 | Scoped Adapter failure, unrelated Route isolation and B3B explicit worker stop/join tests. |
| `NFR-STAB-005` | verified | Gate 1 | Testing/evidence rules and Build Status prohibit unrun soak/hardware claims. |
| `NFR-RT-001` | planned | Gate 7 | First real-time audio callback audit and stress evidence. |
| `NFR-RT-002` | planned | Gate 7 | Fixed-capacity callback-path data structures and overflow tests. |
| `NFR-RT-003` | planned | Gate 7 | Clock-domain timestamps and user-mode recovery/resampling evidence. |
| `NFR-MAINT-001` | verified | Gate 3 | PR #10 passed exact-head Windows/Linux/macOS Rust/Adapter, UI, repository and Windows Tauri hosted checks. |
| `NFR-MAINT-002` | verified | Gate 3 | Cargo dependency boundaries and validator rules include the minimal SensorServer Tungstenite feature set. |
| `NFR-MAINT-003` | verified | Gate 3 | ADRs, compatibility documentation and protocol/Core tests cover foundation public changes. |
| `NFR-MAINT-004` | verified | Gate 5 | Validator-checked SensorServer repository, pinned commit, GPL-3.0-only external-service mode, empty imported paths and distribution risk record. |
| `NFR-MAINT-005` | verified | Gate 1 | `xtask` commands are non-privileged; AGENTS and offline rules exclude deployment operations. |

## Updating this report

Any PRD ID addition, removal or spelling change must update this table in the
same change. `cargo xtask validate-docs` rejects malformed/duplicate PRD IDs,
missing/unknown rows, invalid statuses or Gates, empty evidence, future rows not
marked `planned`, and incomplete Gate 0–3 acceptance evidence.
