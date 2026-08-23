# CAPY-FOUNDATION-002 — Adapter foundation hardening

Status: complete

Owner: Codex

Created: 2026-08-24
Requirements: `FR-ADAPTER-002..006`, `FR-ROUTE-002..008`, `FR-DIAG-001..004`, `FR-PROTO-004`, `FR-PROTO-007`, `NFR-STAB-002..004`, `NFR-MAINT-001..003`

## Objective

Harden the Gate 3 Adapter SDK/Host and repository merge gates so the next task
can implement a real low-rate StandardPort data path without relying on an
unbounded, desynchronized or Mock-specific control foundation.

## Read first

- `AGENTS.md`
- `docs/PRODUCT_REQUIREMENTS.md`
- `docs/ARCHITECTURE.md`
- `docs/DOMAIN_MODEL.md`
- `docs/ADAPTER_MODEL.md`
- `docs/PORT_PROFILES.md`
- `docs/PROTOCOL.md`
- `docs/SECURITY_MODEL.md`
- `docs/TESTING.md`
- `docs/GATE_0_3_REPORT.md`
- `docs/BACKLOG.md`
- nearest directory-specific `AGENTS.md`

## In scope

- truly bounded stdout/stderr line reads and terminal Sidecar Host poisoning;
- generic bounded Route prepare/start/stop/status control contracts;
- deployment-mode-specific Adapter manifest validation;
- Runtime reconciliation when Adapter catalogs invalidate Route endpoints;
- Route backend support validation;
- canonical manifest/schema and Requirement traceability validation;
- stale Gate 3 documentation/UI labels;
- hosted PR CI for Rust, UI, Adapter smoke and Windows Tauri checks.

## Out of scope

- StandardPort data-plane implementation or continuous payload transfer;
- SensorServer, Android, MicYou, Audio Share, VCamdroid or VIIPER;
- drivers, USB/IP, ROS, EasyTier or physical-device work;
- production pairing, networking, signing, packaging or releases.

## Architecture constraints

- Core remains deterministic and independent of process, platform, UI and
  concrete transport APIs.
- Sidecar JSON-RPC remains bounded control traffic only.
- One Route has one Source and one Sink; unrelated Routes remain independent.
- Manifest entrypoints never provide arbitrary shell execution.
- Protocol/channel failures are terminal until a Supervisor creates a new Host.

## Acceptance criteria

1. Oversized newline-free stdout is rejected before allocation can exceed the
   configured line bound; stderr is consumed with a documented bounded policy.
2. Timeout, corrupt response, unexpected ID, closed stdout or oversized response
   poisons and reaps the Host; later requests return a typed poisoned error.
3. Production-facing Adapter APIs contain generic bounded Route contracts and
   no `SmokeSample` return type.
4. InProcess, Sidecar, ExternalService and DriverBacked manifests have explicit,
   mode-appropriate validation and schema coverage.
5. Catalog replacement cannot leave an Active Route pointing at a missing or
   incompatible Port; only dependent Routes change and recovery advances epoch.
6. Unsupported Route backend/interoperability combinations fail with typed Core
   errors.
7. Manifest/schema drift and duplicate/malformed Requirement IDs fail repository
   validation; a Gate 0–3 traceability report records status and evidence.
8. Gate 3 labels/docs are current and PR workflows validate the exact PR head.

## Required tests and evidence

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
corepack pnpm install --frozen-lockfile
corepack pnpm typecheck
corepack pnpm build
cargo check -p capyio-desktop
```

Artifacts to retain:

- `docs/FOUNDATION_HARDENING_REPORT.md`;
- hosted CI workflow definitions and local command results;
- automated unit/integration evidence for each failure boundary.

## Dependency changes

No production dependency is allowed without a separate ADR/dependency note.
Prefer one canonical Rust validator plus explicit schema-drift tests over adding
a new JSON Schema runtime dependency.

## Safety and approvals

- privileged/device operations required: no;
- no driver/APK installation, permission changes or physical-device access;
- no `bcdedit`, `pnputil`, `verifier`, `devcon` or `signtool`;
- no commit, push, PR, merge, tag or publication without a new explicit request.

## Implementation plan

1. Audit current SDK/Host/Runtime/manifest/validation/CI implementations and
   turn each acceptance criterion into a focused regression test.
2. Implement bounded process I/O and terminal Host poisoning/reaping.
3. Replace Mock-specific Route DTOs with generic bounded contracts and update
   mock sidecars/smoke flows.
4. Generalize manifest deployment validation and record the pre-alpha schema
   decision in an ADR.
5. Reconcile catalog/Route epochs and validate Route backend support.
6. Strengthen repository validation, traceability, documentation, UI labels and
   hosted workflows.
7. Run the full local command matrix, fix regressions, write the hardening
   report and move this plan to `completed/` only when exit criteria pass.

## Decisions needed

- Oversized stderr policy: retain a bounded prefix per logical line, discard the
  remainder through the next newline, and continue diagnostics collection.
- Invalidated catalog endpoint policy: transition dependent Routes to Offline,
  attach a structured Problem and advance epoch; compatible restoration keeps
  the Route Offline until explicitly restarted.
- Manifest validation source: Rust semantic validation is canonical for
  cross-field rules; committed schema structural expectations are enforced by
  validator parity tests.

## Risks

- Killing a timed-out child must remain deterministic on Windows, Linux and
  macOS even though only Windows is locally executable here.
- Manifest changes are intentionally pre-alpha breaking but must keep schema,
  examples, docs and Rust types synchronized.
- Hosted CI can be configured locally, but GitHub Runner success is unavailable
  until an explicitly authorized push/PR.

## Completion record

Implemented:

- bounded Sidecar reads and terminal poisoned/reaped Host semantics;
- generic bounded Route controls and Mock-private finite payload fixture;
- Adapter Manifest v2 mode bindings, schema parity tests and ADR 0020;
- typed backend/interoperability validation and catalog reconciliation;
- 84-ID PRD traceability, exact-head workflows and current Gate 3 labels.

Validation:

- full repository validator self-test and 84-ID traceability validation passed;
- Rust format/check/Clippy and 70 workspace tests passed;
- manifest validation, Adapter Smoke and full `cargo xtask ci` passed;
- frozen pnpm install/typecheck/build and Windows Tauri check/build passed;
- detailed commands and counts are in `docs/FOUNDATION_HARDENING_REPORT.md`.

Not validated:

- hosted runner results, Linux/macOS local process behavior and all
  hardware/platform-device behavior.

Follow-up issues:

- begin the fixture-first `CAPY-IMU-001` product slice only after this plan is
  reviewed, committed and its hosted checks are observed.
