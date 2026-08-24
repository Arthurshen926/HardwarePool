# CAPY-IMU-001B0 — SensorServer parser and IMU pairing contract

Status: complete

Owner: Codex

Created: 2026-08-24

Completed: 2026-08-24

Requirements: `FR-SCEN-007`, `FR-ADAPTER-005`, `FR-PORT-002..005`, `FR-ROUTE-005`, `NFR-SEC-005`, `NFR-STAB-002..005`, `NFR-MAINT-004`

## Objective

Convert bounded, recorded SensorServer accelerometer/gyroscope JSON messages
into deterministic `capyio.motion.imu-samples/1` envelopes while preserving
component source timestamps and making skew/regression explicit. No socket or
phone is required for this slice.

## Read first

- `AGENTS.md`
- `docs/PRODUCT_REQUIREMENTS.md`
- `docs/ARCHITECTURE.md`
- `docs/ADAPTER_MODEL.md`
- `docs/PORT_PROFILES.md`
- `docs/PROTOCOL.md`
- `docs/SECURITY_MODEL.md`
- `docs/THIRD_PARTY_STRATEGY.md`
- `docs/adr/0019-third-party-vertical-slice-reuse.md`
- `docs/adr/0021-gate-5-external-sensorserver-lab.md`

## In scope

- verified upstream repository, commit and GPL-3.0-only provenance;
- a CapyIO-authored parser for documented per-sensor JSON messages;
- fixed input, axis, timestamp, skew and sequence bounds;
- deterministic pairing of asynchronous acceleration and gyroscope readings;
- optional, freshness-bounded magnetic field inclusion;
- preservation of per-component timestamps in the IMU Profile;
- abnormal fixtures and hardware-free tests.

## Out of scope

- WebSocket/TCP/DNS/mDNS, reconnection or authentication;
- importing, building, installing or launching SensorServer;
- live phone data, performance or background-lifecycle claims;
- desktop live-state wiring, 3D rendering, audio, gamepad or drivers.

## Architecture constraints

- third-party parsing stays in the Adapter crate, not Core or generic Runtime;
- the generic data-plane crate contains only Profile semantics and no network;
- every untrusted allocation is preceded by a byte/count bound;
- asynchronous sensors are paired without changing their original timestamps;
- unsupported accuracy, timestamp regression and excessive skew are explicit.

## Acceptance criteria

1. The official three-field message shape maps exact finite axes and Android
   accuracy values; malformed, unknown, oversized and wrong-length input fails.
2. Acceleration and gyroscope may arrive in either order and emit only when both
   exist within the configured skew bound.
3. Component timestamps remain in the output and the envelope source timestamp
   is their maximum; timestamps are never silently rewritten.
4. Regression and excessive skew have typed outcomes; a later valid reading can
   recover without restarting unrelated consumers.
5. Output sequence is monotonic and refuses exhaustion.
6. No upstream source/binary, socket or new external dependency enters the tree.

## Required tests and evidence

```text
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask validate-docs
cargo xtask ci
```

Artifacts to retain:

- recorded synthetic SensorServer message fixtures;
- parser/pairing tests;
- updated upstream provenance and ADR.

## Dependency changes

No new external production dependency. The crate uses existing workspace
Serde/JSON, error and CapyIO data-plane dependencies. A WebSocket library is a
separate reviewed slice.

## Safety and approvals

- privileged/device operations required: no;
- approved target: none used in this slice;
- forbidden operations: network connection, APK install, permission/service
  change, device mutation, driver tools, package/release/tag/PR publication;
- preserve `CapyIO_Codex_Master_Prompt.md` and ignored Android evidence.

## Implementation plan

1. Resolve the normative Gate 5 scope conflict and pin provenance.
2. Extend IMU Profile v1 with optional per-component source timestamps.
3. Implement bounded parsing and deterministic pairing.
4. Add official-shape and abnormal fixtures/tests.
5. Update Profile/testing/traceability docs and run all local gates.

## Decisions needed

- Pairing policy: latest readings pair only when their timestamps differ by no
  more than a configured fixed limit; otherwise return an explicit skew outcome.
- Combined accuracy: use the least accurate required component.
- Calibration: report `Raw`; SensorServer does not document calibration state.
- Envelope timestamp: maximum of included component timestamps, with originals
  retained inside the Profile payload.

## Risks

- SensorServer may evolve its JSON shape; pinned provenance and strict parsing
  make drift visible rather than silently accepting changed semantics.
- Pairing is not sensor fusion and does not prove physical synchronization.
- GPL process/protocol separation is an engineering boundary, not legal advice.

## Completion record

Implemented:

- resolved the v0.3 non-goal versus active Gate 5 scope conflict through ADR
  0021 and the v0.4-pre-alpha PRD;
- pinned SensorServer upstream/license provenance without importing source or a
  binary;
- added optional component timestamps to IMU Profile v1;
- implemented bounded strict JSON parsing and deterministic required-component
  pairing with explicit replacement, skew, regression and exhaustion behavior;
- added nine contract tests and two recorded synthetic messages.

Validation:

- full workspace format/check/Clippy and 88 Rust tests passed;
- repository validation traced 84 IDs and checked SensorServer provenance;
- manifests, Adapter Smoke, fixture replay, frontend typecheck/build and
  `cargo xtask ci` passed.

Not validated:

- live SensorServer, WebSocket and physical device.

Follow-up issues:

- `CAPY-IMU-001B1`: reviewed WebSocket client and mock server.
