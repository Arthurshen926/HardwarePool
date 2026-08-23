# CAPY-FOUNDATION-001 — CapyIO foundation migration

Status: active  
Owner: Codex  
Created: 2026-08-23  
Source: `CapyIO_Codex_Master_Prompt.md`, PRD v0.2, Architecture v0.1

> The master prompt requested plan number `0002`, but that identifier already
> belongs to the completed HP-CORE-003 plan. This plan uses the next available
> repository sequence number instead of creating ambiguous duplicate IDs.

## Goal

Migrate the verified HardwarePool bootstrap into the CapyIO pre-alpha
foundation without implementing real hardware access. The resulting repository
must demonstrate symmetric nodes, typed capability ports, independent routes,
and an isolated mock Adapter sidecar.

## In scope

- Gate 0: baseline audit and reproducible validation evidence;
- Gate 1: CapyIO user-visible, crate, directory, package and Protobuf naming;
- Gate 1: PRD v0.3, architecture, domain/Adapter/UX/third-party documents,
  roadmap, backlog, ADRs and Agent rules;
- Gate 2: `Node`, `AdapterInstance`, `Capability`, `Port`, `Route`, `Session`
  and `Problem` Core models;
- Gate 2: route compatibility and lifecycle validation, Runtime orchestration,
  deterministic fixtures, protocol round trips and Mock UI;
- Gate 3: Adapter manifest schema, Rust Adapter SDK, NDJSON control codec,
  process supervisor, mock source/sink sidecars and smoke tests;
- unified validation commands and evidence.

## Out of scope

- real microphone, speaker, camera, IMU or input data paths;
- Android project generation, APK installation, permissions or foreground
  services;
- Windows drivers, virtual devices, WDK operations or driver deployment;
- vendoring MicYou, Audio Share, VCamdroid, SensorServer, VIIPER or other
  third-party source;
- production network transport, pairing, encryption, MCAP, ROS, EasyTier,
  WebRTC, RTSP, FFmpeg or USB/IP;
- releases, tags, pull requests or repository publication.

## Architecture constraints

1. Core remains deterministic, pure Rust and free of platform SDKs, I/O,
   concrete transports and UI frameworks.
2. Nodes have no global provider/consumer role; direction belongs to Ports.
3. A Route connects exactly one compatible Source Port to one Sink Port.
4. Opposite-direction Routes are independent state machines.
5. Projections, Panels and Recorders are represented through Adapter-owned
   Capabilities/Ports rather than new top-level Core object families.
6. Sidecar control uses NDJSON on stdin/stdout; ordinary logs use stderr;
   high-bandwidth payloads never use this control channel.
7. Retained queues, messages and diagnostics are bounded.
8. UI controls a Runtime and never owns its long-lived lifecycle.

## Allowed dependency changes

- Reuse existing workspace dependencies where possible.
- A new production dependency requires a recorded purpose, license,
  maintenance note and alternatives before it is added.
- No GPL source is imported in Gates 0–3.

## Safety

- no driver commands or system boot/security changes;
- no APK installation or Android permission/service changes;
- no physical-device access;
- no production credentials or signing material;
- no remote push, release or PR as required by the current master task.

## Acceptance criteria

### Gate 0

- [x] Repository, specifications, Core, Runtime, Protocol and UI audited.
- [x] Baseline commands executed and recorded in `docs/BASELINE_REPORT.md`.
- [x] No missing component was installed as part of the audit.

### Gate 1

- [x] User-visible and active code names are CapyIO / `capyio-*` / `capyio.v1`.
- [x] PRD v0.3 has stable IDs for every fixed scenario and invariant.
- [x] Required architecture, model, UX, third-party and roadmap documents exist.
- [x] ADRs 0009–0019 capture the migration decisions without rewriting history.
- [x] README labels the project pre-alpha and states that real Adapters are absent.

### Gate 2

- [ ] `NodeRole`, `LocalRole`, `StreamRole` and Binding/Projection-centric Core
  flows no longer define the active model.
- [ ] Capability owns one or more typed Ports and Adapter ownership is explicit.
- [ ] Route rejects Source→Source, Sink→Sink and incompatible Profiles.
- [ ] Route states implement Draft, Prepared, Starting, Active, Stopping,
  Stopped, Failed and Offline with invalid-transition tests.
- [ ] Opposite-direction Routes coexist and stopping one does not affect another.
- [ ] Protocol round trips Node Catalog, Capability/Port Catalog, Route and Problem.
- [ ] Mock UI exposes Quick Actions and Workspace with four independent Routes.

### Gate 3

- [ ] Adapter manifest Rust types validate against the committed JSON Schema.
- [ ] NDJSON codec handles framing, malformed messages and response correlation.
- [ ] Mock sidecars keep normal logs on stderr and control messages on stdout.
- [ ] Adapter Host can initialize, probe, read a catalog, prepare/start/stop a
  Route and shut a child process down.
- [ ] Unexpected child exit only fails the owning Adapter and Routes.
- [ ] `cargo xtask validate-manifests` and `cargo xtask adapter-smoke` pass.

## Required validation

```text
python scripts/validate_repository.py
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask validate-docs
cargo xtask validate-manifests
cargo xtask adapter-smoke
cargo xtask ci
pnpm typecheck
pnpm build
```

## Milestones and evidence

| Gate | Status | Evidence |
|---|---|---|
| 0 — baseline | complete | `docs/BASELINE_REPORT.md` |
| 1 — naming/docs | complete | repository validator, Rust check and UI build |
| 2 — domain/runtime/UI | pending | Rust tests, protocol tests, UI build |
| 3 — Adapter SDK/sidecars | pending | manifest validation and smoke test |

## Known risks

- This is an intentional breaking pre-alpha model and Protobuf package rename.
- Renaming directories and crates touches most of the small repository; changes
  must remain split into buildable Gates.
- The existing host-operation seam is audio-shaped and may need a compatible
  generalization rather than deletion.
- Child-process tests can be sensitive to Windows process and pipe behavior.
- The local Codex process PATH differs from the persisted user environment;
  validation commands must use a documented session PATH until a new shell is
  opened.

## Completion record

Implemented:

- Gate 0 baseline audit and report;
- Gate 1 CapyIO naming, repository layout, PRD v0.3, architecture, ADR and
  third-party provenance skeleton.

Validation:

- See `docs/BASELINE_REPORT.md`;
- Gate 1: repository validation, `cargo check --workspace`, pinned pnpm
  typecheck and production build passed on 2026-08-23.

Not validated:

- Gates 2–3 are still in progress;
- no hardware, driver, APK, transport or production security behavior.
