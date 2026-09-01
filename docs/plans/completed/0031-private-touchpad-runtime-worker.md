# CAPY-PTP-002G — Deterministic Runtime worker boundary

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`,
`NFR-STAB-001..004`, `NFR-SEC-001..003`, `NFR-MAINT-001..003`

## Objective

Replace caller-supplied Route snapshots and timestamps at the private touchpad
packet/tick boundary with a deterministic worker that reads current state from
composition-owned interfaces.

## In scope

- Core-only read-only Route provider returning one owned snapshot per action;
- one coherent local authorization/ingress clock sample per action;
- construction, activation, enqueue, tick, later-epoch transition and stop;
- rollback detection for both millisecond and nanosecond clock values;
- fail-closed Route-provider, clock, Route-state and session errors;
- fixed saturating counters without an event/log queue;
- real `NodeRuntime` lifecycle verification through dev-only dependencies;
- pure memory Sink and no platform/device/network operations.

## Out of scope

- background thread, async executor, timer or UI command registration;
- authenticated transport, sockets, pairing, encryption or reconnect;
- Android runtime/JNI/Gradle/APK integration;
- construction of a Windows synthetic device or native submission;
- driver/VHF work and physical multi-finger acceptance.

## Acceptance criteria

1. A packet or tick caller cannot supply a Route snapshot or timestamp.
2. Each live action uses one coherent clock sample and one owned current Route
   snapshot before entering the existing bounded session.
3. Clock rollback in either domain and provider/clock failure fail closed.
4. Stop can close the Sink even if provider/clock access is unavailable.
5. A real `NodeRuntime` Starting/Active/Stopping/Stopped and later-epoch
   lifecycle is covered without a production Adapter to Runtime dependency.
6. Tests perform no network, thread, timer, Windows device or desktop input.

## Dependency changes

No production or external dependency is added. Existing local `capyio-runtime`
and `capyio-testkit` crates are dev-only dependencies used to prove composition
against the real Runtime catalog and Route state machine.

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

## Completion evidence

- `PrivateTouchpadRuntimeWorker` owns provider, clock and Route session and
  exposes caller-driven activate/enqueue/tick/advance/stop commands.
- A single immutable Route snapshot prevents mid-action lifecycle drift.
- Provider/clock failure and either clock rollback close/poison the session;
  Runtime Stopping is detected on the next tick.
- Five hardware-free tests use a real `NodeRuntime` plus memory Sink for start,
  packet/tick, epoch recovery, stop and failure behavior.
- Focused checks and full repository CI pass without device/network actions.

Detailed evidence: `docs/CAPY_PTP_002G_REPORT.md`.
