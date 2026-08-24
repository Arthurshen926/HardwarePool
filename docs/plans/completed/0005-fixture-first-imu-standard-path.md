# CAPY-IMU-001A — Fixture-first IMU StandardPort path

Status: complete

Owner: Codex

Created: 2026-08-24

Completed: 2026-08-24

Requirements: `FR-SCEN-007`, `FR-SCEN-009`, `FR-CAP-006`, `FR-PORT-002..005`, `FR-ROUTE-005`, `NFR-STAB-002..005`

## Objective

Prove a bounded, transport-independent `capyio.motion.imu-samples/1` data path
from a deterministic fixture to two independent consumers: a numeric Panel and
a JSONL Recorder. Retain a sanitized, read-only Android lab baseline without
claiming that a phone or APK supplied the data path.

## Read first

- `AGENTS.md`
- `docs/PRODUCT_REQUIREMENTS.md`
- `docs/ARCHITECTURE.md`
- `docs/DOMAIN_MODEL.md`
- `docs/ADAPTER_MODEL.md`
- `docs/PORT_PROFILES.md`
- `docs/DATA_PLANE.md`
- `docs/PROTOCOL.md`
- `docs/SECURITY_MODEL.md`
- `docs/TESTING.md`
- `docs/BACKLOG.md`
- `docs/LOCAL_LAB.md`
- nearest directory-specific `AGENTS.md`

## In scope

- a pure Rust, transport-independent `capyio-data-plane` crate;
- bounded envelopes, queues, fan-out and explicit sequence/epoch/gap outcomes;
- an IMU Profile v1 semantic type and deterministic recorded-style fixture;
- independent numeric Panel and bounded JSONL Recorder consumers;
- `cargo xtask android-doctor`, `android-baseline` and `android-collect`;
- an explicit device serial, read-only ADB allow-list, sanitized evidence and
  no implicit selection when multiple devices are visible;
- minimal simulated UI state for the fixture Panel and Recorder Routes;
- unit, integration, replay and abnormal-path evidence that requires no phone.

## Out of scope

- SensorServer integration or any live sensor payload capture;
- building, signing, installing or granting permissions to an Android APK;
- production networking, pairing, authentication or encryption;
- 3D rendering, MCAP, ROS 2, gamepad/VIIPER, haptics or audio;
- Windows virtual devices, driver installation or privileged host changes;
- claiming physical-phone end-to-end success from ADB inventory alone.

## Architecture constraints

- Core remains independent of Profile payloads, transport, Android and UI.
- The data-plane crate opens no sockets and selects no concrete transport.
- Every retained queue, payload and diagnostic has a declared bound.
- Epoch changes and sequence gaps are explicit; timestamps are never silently
  rewritten to hide loss, restart or disconnect.
- Panel and Recorder have separate queues and lifecycle state. Stopping or
  overflowing one does not stop or corrupt the other.
- Android commands are read-only, require `--serial`, and write only sanitized
  evidence below ignored `test-results/` paths.

## Acceptance criteria

1. A versioned IMU envelope preserves source and receive timestamps, clock
   domain, epoch, sequence, units, coordinate frame, accuracy and sensor
   metadata with bounded payloads and metadata.
2. Wrong profile, stale epoch, duplicate/late sequence, gap and queue overflow
   return explicit typed outcomes or Problems; no silent repair occurs.
3. Deterministic replay feeds Panel and Recorder independently, and both Routes
   can be stopped/restarted without mutating the other consumer.
4. A slow or full consumer applies its own documented overflow policy without
   blocking producer progress or another consumer.
5. Recorder output is bounded per line, deterministic, valid JSONL and rejects
   unsafe output paths or unbounded retention.
6. The desktop UI visibly labels the IMU flow as simulated/fixture data and
   does not claim a phone connection.
7. Android doctor/baseline/collect fail safely for missing ADB, missing serial,
   unauthorized/offline devices or ambiguous targets; baseline evidence is
   sanitized and read-only.
8. All fixture tests and repository gates run without a phone. Any phone
   baseline is reported separately from data-plane acceptance.

## Required tests and evidence

```text
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask validate-docs
cargo xtask validate-manifests
cargo xtask adapter-smoke
cargo xtask demo
cargo xtask ci
corepack pnpm install --frozen-lockfile
corepack pnpm typecheck
corepack pnpm build
cargo check -p capyio-desktop
cargo xtask android-doctor --serial <explicit-serial>
cargo xtask android-baseline --serial <explicit-serial>
```

Artifacts to retain:

- `fixtures/imu/imu_samples_v1.jsonl`;
- focused data-plane and consumer test output;
- ignored, sanitized `test-results/android/<timestamp>/` evidence;
- `docs/CAPY_IMU_001A_REPORT.md`.

## Dependency changes

No new production dependency is allowed for this slice. Use existing workspace
Serde/JSON support and the standard library. A future live SensorServer or
transport dependency requires a separate task and dependency record.

## Safety and approvals

- privileged/device operations required: no;
- approved target: explicit ADB serial supplied by the user, read-only inventory
  only in this slice;
- forbidden operations: APK install/uninstall, permission grants, foreground
  service changes, device reset, settings mutation, driver tools, production
  signing, release publication and unsanitized device identifiers in Git;
- preserve the untracked user file `CapyIO_Codex_Master_Prompt.md`.

## Implementation plan

1. Define the bounded generic envelope, validation and per-consumer queue/fanout
   semantics with focused tests.
2. Define IMU Profile v1, deterministic fixture parsing and abnormal fixtures.
3. Implement independent Panel and Recorder sinks and replay integration tests.
4. Add safe Android doctor/baseline/collect commands and sanitized evidence.
5. Expose fixture-only state in the Mock backend/UI with explicit labels.
6. Update Profile/data-plane/testing/lab/traceability docs, run all gates and
   write the evidence report.

## Decisions needed

- Queue overflow policy: reject the incoming envelope for that consumer and
  increment a bounded dropped counter; never evict an earlier accepted item.
- Gap policy: accept an ahead sequence with an explicit preceding gap outcome;
  late/duplicate samples remain rejected and observable.
- Recorder policy: a caller supplies a repository-relative evidence path and a
  fixed maximum record count; the fixture adapter never exposes arbitrary
  filesystem access through UI commands.

## Risks

- A generic envelope can become a second protocol if wire layout or transport
  concerns leak into it; this slice keeps it semantic and in-process only.
- Android vendor output contains unstable or identifying fields; collection
  must allow-list fields and sanitize before writing evidence.
- A visually useful fixture UI can be mistaken for live hardware; labels and
  report language must preserve the distinction.

## Completion record

Implemented:

- bounded generic envelope, queue, epoch/gap/overflow and fan-out semantics;
- IMU Profile v1, deterministic fixture, numeric Panel and JSONL Recorder;
- headless replay plus Browser/Tauri schema-v3 fixture summary;
- explicit-serial Android doctor/baseline/collect with sanitized evidence.

Validation:

- full workspace fmt/check/Clippy and 79 Rust tests passed;
- repository validation traced 84 IDs; manifests, Adapter Smoke and xtask CI
  passed;
- frozen frontend typecheck/build and Windows Tauri check/build passed;
- read-only Android doctor/baseline/collect passed; retained evidence contains
  no serial, address/port or build fingerprint.

Not validated:

- live SensorServer, APK, wireless sensor stream and physical IMU timing.

Follow-up issues:

- `CAPY-IMU-001B`: live SensorServer Adapter and explicitly authorized APK lab.
