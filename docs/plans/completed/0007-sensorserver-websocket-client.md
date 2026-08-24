# CAPY-IMU-001B1 — Bounded SensorServer WebSocket client

Status: complete

Owner: Codex

Created: 2026-08-24

Requirements: `FR-ADAPTER-005`, `FR-PORT-002..005`, `FR-DIAG-001`, `NFR-SEC-005`, `NFR-STAB-002..005`, `NFR-MAINT-004`

## Objective

Connect one fixed SensorServer sensor endpoint through a bounded synchronous
WebSocket client and prove the real RFC 6455 path against a local mock server,
without using a phone or claiming production security.

## In scope

- user-approved `tungstenite` 0.30.0 dependency record;
- IP-literal, fixed-path `ws://` local-lab endpoint validation;
- connect/read/write timeouts and bounded frame/message configuration;
- text-message parsing through the existing strict SensorServer parser;
- ping/pong/close and binary/oversize/error outcomes;
- loopback mock server integration tests;
- reconnect/session boundary ready for later epoch orchestration.

## Out of scope

- DNS, discovery, TLS, credentials, redirects or production security;
- connection to the physical phone in automated tests;
- APK installation, SensorServer service launch or Android permissions;
- long-running Runtime orchestration, UI state or automatic retry;
- audio, gamepad and drivers.

## Architecture constraints

- network code stays inside the SensorServer Adapter;
- Core, generic data plane and Tauri UI gain no socket dependency;
- the client never sends sensor payloads through Adapter JSON-RPC;
- every connect/read/write has a deadline and every retained buffer is bounded;
- a dropped connection never silently resumes the prior stream epoch.

## Acceptance criteria

1. Only a validated IP/port and known sensor kind can form a URL.
2. The client rejects oversized handshake/data, binary messages and malformed
   JSON without yielding a reading.
3. A valid text frame yields the exact validated source timestamp and axes.
4. Ping is serviced, close is explicit, and read timeout is distinguishable
   from a terminal connection failure.
5. Loopback tests cover success, timeout, close, binary and oversize behavior.
6. No async/TLS dependency, phone action or production-security claim is added.

## Required tests and evidence

```text
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask validate-docs
cargo xtask ci
```

## Dependency changes

`tungstenite` 0.30.0, `MIT OR Apache-2.0`, default features disabled,
`handshake` only. Purpose, alternatives, transitive scope and security boundary
are recorded in ADR 0022.

## Safety and approvals

- production dependency: explicitly approved by the user;
- physical-device operation required: no for this slice;
- forbidden: APK/permission/service change, credential storage, arbitrary URI,
  driver tools, release/tag/PR publication;
- preserve the user's untracked master prompt and ignored Android evidence.

## Implementation plan

1. Add the pinned minimal dependency and dependency record.
2. Implement endpoint/configuration/client outcome types.
3. Add loopback RFC 6455 server tests for normal and abnormal frames.
4. Update testing/security/provenance evidence and run all gates.

## Decisions needed

- Blocking client is sufficient because each connection is worker-owned and
  SensorServer uses separate streams per sensor.
- No automatic reconnect in this slice; the caller decides when to advance
  epoch and create a fresh client.

## Risks

- Blocking APIs must never run on UI or real-time callbacks.
- Tailscale transport protection is not CapyIO application authorization.
- SensorServer may close or change paths; strict outcomes must remain visible.

## Completion record

Implemented:

- minimal `tungstenite` 0.30.0 dependency and provenance record;
- typed IP-literal endpoint, bounded connection configuration and fixed sensor paths;
- worker-owned synchronous client with explicit open/closed/failed state;
- strict text mapping plus typed control, timeout, close and error outcomes;
- seven loopback integration tests using real RFC 6455 framing.

Validation:

- `cargo fmt --all -- --check`: pass;
- workspace check and Clippy with warnings denied: pass;
- `cargo test --workspace --exclude capyio-desktop`: 95 pass;
- repository/docs/manifests/Adapter Smoke/fixture/frontend gates: pass;
- `cargo xtask ci`: pass with Node.js on `PATH`.

Not validated:

- physical SensorServer and live Runtime/UI data.

Follow-up issues:

- `CAPY-IMU-001B2`: authorized physical SensorServer lab.
