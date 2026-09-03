# CAPY-PTP-002R — Bounded host channel composition

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`, `NFR-STAB-001..004`,
`NFR-SEC-001..003`, `NFR-PERF-001..003`, `NFR-MAINT-001..003`

## Objective

Provide a concrete bounded admitted channel that composes the Runtime-driven
touchpad sender and receiver paths without prematurely selecting or claiming a
cross-device authenticated transport.

## In scope

- preallocated 1..=64 packet in-process host queue;
- explicit admission controller, sender and receiver ownership handles;
- definite pre-write rejection on full queue or known disconnect;
- ordered drain after normal sender close;
- stale queue discard on denial, binding replacement and receiver close;
- fixed saturating metrics and idempotent endpoint close;
- real `NodeRuntime` sender → channel → receiver → Windows projector test.

## Out of scope

- sockets, stream framing, peer discovery or reconnect;
- pairing, identity verification, authenticated encryption, keys or
  cryptographic replay protection;
- cross-thread scheduling or arbitrary StandardPort interoperability;
- Android OS/JNI, APK, physical devices or Windows desktop input.

## Acceptance criteria

1. Capacity is fixed and validated before handles are returned.
2. Full/disconnected sends are known not to write and therefore retryable.
3. Lifecycle or binding changes cannot leak stale queued contact packets.
4. Accepted packets remain ordered and may drain after sender close.
5. Both Runtime workers compose through the concrete channel into Windows
   projection using one real local Route.
6. Targeted tests, Clippy, full CI and repository validation pass.

## Required validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_host_channel
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

## Completion evidence

- Two channel lifecycle/bounds tests pass.
- Thirteen ordinary Runtime-worker tests pass; five separately authorized
  Windows native-input tests remain ignored.
- Cancel, active contact and release cross both workers and reach the hardware-
  free Windows projector in order.
- Full repository CI and documentation validation pass.
- No dependency, socket, public wire claim or device operation was added.

Detailed evidence: `docs/CAPY_PTP_002R_REPORT.md`.
