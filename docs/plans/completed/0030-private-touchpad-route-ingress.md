# CAPY-PTP-002F — Authorized Route ingress scheduler

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`,
`NFR-STAB-001..004`, `NFR-SEC-001..003`, `NFR-MAINT-001..003`

## Objective

Bind the private touchpad receiver to one current Core `Route` authorization
snapshot and place a fixed-capacity, explicitly pumped ingress queue between a
future transport callback and the Sink lifecycle.

## In scope

- validation of Route ID/session/endpoints, local Sink Port, touchpad Profile,
  AdapterManaged backend, authorization expiry, live state and matching epoch;
- distinct Starting/Active/TimedOut/Failed/Closed ingress states;
- fixed-capacity preallocated packet queue with a maximum of 64 records;
- monotonic trusted-local enqueue/pump clock and maximum packet-size checks;
- bounded pump that drains at most the configured queue capacity and then polls
  the receiver active-contact timeout even when no packet arrived;
- rejection of a queued record that reaches the active-idle deadline before it
  can be pumped, so stale contact input never briefly reaches the Sink;
- close/poison on Route invalidation, authorization expiry, queue overflow,
  packet oversize, clock regression or receiver failure;
- explicit activation, disconnect and strictly later Route-epoch transition;
- deterministic Core Route/fake Sink/Windows projector tests only.

## Out of scope

- choosing or opening TCP, UDP, QUIC, WebSocket or another transport;
- pairing, identity keys, encryption or production authorization issuance;
- Runtime worker threads, async runtime or UI commands;
- Android JNI/Gradle/APK work;
- Windows device creation, native input submission, driver/VHF work;
- physical gesture acceptance.

## Acceptance criteria

1. A session cannot bind unless the Route is authorized, unexpired,
   AdapterManaged, touchpad-profiled, Starting/Active, pointed at the expected
   Sink and epoch-equal to the input stream.
2. A Starting binding accepts no packets until the current Route becomes
   Active and activation is explicit.
3. Queue capacity is configured as 1..=64, preallocated once, and overflow
   closes the receiver rather than dropping an unknown active state.
4. Each enqueue and pump revalidates current Route identity/state/authorization;
   expiry, stop/offline or epoch mismatch closes the Sink.
5. Local clock regression and packets above 152 bytes fail closed.
6. One pump processes at most the queue capacity in order, rejects records that
   reached the active-idle deadline and always polls the timeout afterward.
7. A strictly later Starting Route epoch discards queued old-epoch packets and
   advances the receiver/Sink before reactivation.
8. Tests perform no network, device or OS-input operation.

## Required validation

```text
cargo fmt --all -- --check
cargo check -p capyio-remote-touchpad-adapter --all-targets
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo test -p capyio-remote-touchpad-adapter
cargo xtask validate-docs
cargo xtask ci
git diff --check
```

## Dependency changes

Add only the existing local `capyio-core` crate as a direct dependency so the
Adapter can validate the normative `Route` snapshot and typed IDs. No external
dependency or Runtime/platform SDK edge is added.

## Safety

- Route authorization is an input to this boundary, not granted by it.
- Queue overflow and lifecycle mismatch cancel rather than silently discard.
- The queue stores only the fixed 152-byte private record and local arrival
  timestamp; no unbounded payload or raw user content is retained.
- Pumping is caller-driven and bounded; no background thread is created.
- No command in this slice invokes user32 or opens a socket.

## Implementation plan

1. Define Route binding, ingress limits/states/faults and fixed packet record.
2. Implement activation, enqueue, pump, epoch transition and close behavior.
3. Add deterministic authorization/queue/timeout/epoch/failure tests.
4. Update architecture, security, protocol, testing and traceability evidence.
5. Run full validation and archive this plan.

## Completion evidence

- `PrivateTouchpadRouteSession` binds the exact Core Route/Session/endpoints,
  touchpad Profile, AdapterManaged backend, grant expiry and stream epoch.
- A preallocated logical 1..=64 record queue supports explicit activation,
  ordered bounded pump, empty-pump timeout and later-epoch transition.
- Overflow, stale residence, oversize, clock regression, Route invalidation,
  authorization expiry and receiver failure clear/close/poison fail-closed.
- Ten deterministic tests cover the binding/scheduler and a hardware-free
  Core Route → queue → receiver → Windows projector lifecycle.
- Focused checks and full `cargo xtask ci` pass; no socket, user32 device or
  desktop injection operation ran.

Detailed evidence: `docs/CAPY_PTP_002F_REPORT.md`.
