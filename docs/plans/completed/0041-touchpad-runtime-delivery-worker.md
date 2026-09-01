# CAPY-PTP-002Q — Runtime-driven sender delivery worker

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`, `NFR-STAB-001..004`,
`NFR-SEC-001..003`, `NFR-PERF-001..003`, `NFR-MAINT-001..003`

## Objective

Make trusted Runtime Route state and a coherent local clock authoritative for
every private touchpad sender delivery operation, rather than accepting Route
snapshots or timestamps from the Android/packet-facing caller.

## In scope

- a deterministic caller-driven sender worker above the admitted-channel
  delivery session;
- fresh Runtime-owned Route snapshot and trusted-clock sample at construction,
  before every frame and before normal close;
- exact Active Source, Route, Session, Sink, epoch and authorization-expiry
  binding derived by the Adapter;
- fail-closed clock rollback, provider/clock failure, authorization expiry,
  Route stop and binding drift;
- fixed-size saturating metrics and exactly-once admitted-channel cleanup;
- integration tests using a real local `NodeRuntime` and fake channel.

## Out of scope

- pairing, identity verification, authenticated encryption, keys or replay
  protection;
- concrete socket/transport implementation, reconnect or scheduling thread;
- Kotlin/JNI, Android components, permissions, APK or physical devices;
- Windows synthetic-device creation or desktop input.

## Acceptance criteria

1. Callers cannot supply Route snapshots or timestamps to sender delivery.
2. Construction, every frame and normal close read current Runtime/clock state.
3. Only the exact active, authorized, unexpired Source binding is accepted.
4. Route/lease/clock/provider drift faults and closes before more delivery.
5. Delivery ambiguity and transactional retry semantics from 002P remain intact.
6. Tests use actual Runtime Route transitions without network or device effects.
7. Targeted tests, Clippy, full CI and repository validation pass.

## Required validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker
cargo test -p capyio-remote-touchpad-adapter
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

## Completion evidence

- Twelve ordinary Runtime-worker tests pass; five separately authorized native
  Windows tests remain ignored by default.
- Three delivered frames plus close each use fresh Runtime/clock context.
- Expiry, Route stop, inactive construction, provider/clock failure and rollback
  close fake channels without an unadmitted packet.
- Adapter-wide tests and Clippy pass with warnings denied.
- Full repository CI and documentation validation pass.
- No dependency, socket, cryptographic claim or device operation was added.

Detailed evidence: `docs/CAPY_PTP_002Q_REPORT.md`.
