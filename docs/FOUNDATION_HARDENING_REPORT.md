# CAPY-FOUNDATION-002 Completion Report

> Date: 2026-08-24
>
> Branch: `codex/capyio-foundation`
>
> Base commit: `f498a90`
>
> Scope: local working tree only; no commit, push, pull request or release was
> created by this task.

## Outcome

The Gate 3 Adapter/control foundation is hardened enough to start a separately
planned low-rate StandardPort data-path task. This task did not implement that
data path, a real Adapter, Android software, an APK, a Windows virtual endpoint
or any hardware integration.

The completed foundation now provides:

- bounded Sidecar stdout/stderr reads, including newline-free overflow;
- terminal Host poisoning and child reaping after protocol desynchronization;
- generic, bounded Route prepare/start/stop/status control contracts;
- deployment-mode-specific Adapter Manifest v2 bindings;
- typed Route backend/interoperability rejection;
- catalog reconciliation with scoped Route invalidation and epoch advancement;
- checked traceability for all 84 normative PRD Requirement IDs;
- exact-PR-head workflows for Rust, Adapter smoke, UI and Windows Tauri gates.

## Acceptance evidence

| Criterion | Result | Evidence |
|---|---|---|
| Bounded stdout/stderr | passed | Host integration tests reject newline-free stdout above 64 KiB and retain at most a 2 KiB marked stderr prefix while draining the logical line. |
| Terminal Host failure | passed | Timeout, malformed response, unexpected ID, oversized response and stdout closure poison/reap the child; subsequent requests return `SidecarPoisoned`. |
| Generic Route control | passed | SDK round-trip/bound tests and Adapter smoke use generic Route DTOs; the finite sample is private to the Mock Adapter. |
| Deployment-mode manifests | passed | Manifest v2 Rust/schema tests cover InProcess, Sidecar, ExternalService and DriverBacked bindings and explicitly reject v1/unknown versions and unknown fields. |
| Catalog reconciliation | passed | Runtime tests invalidate only dependent Routes, attach a structured Problem, advance epoch and require explicit restart after compatible restoration. |
| Backend compatibility | passed | Core tests reject unsupported Adapter route modes, cross-node LocalPipeline and incompatible interoperability with typed errors. |
| Traceability/validation | passed | The repository validator self-tests malformed, duplicate and non-canonical IDs and checks 84 PRD rows plus nine Gate 0-3 acceptance rows. |
| Exact-head merge gates | configured | Workflow source checks out the exact pull-request head and defines three-OS Rust/Adapter, frozen UI and Windows Tauri gates. Hosted results for this head are pending. |

## Local validation

All final commands below passed on the Windows development host:

```text
python scripts/validate_repository.py --self-test
python scripts/validate_repository.py --validate-docs
python scripts/validate_repository.py
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                              # 70 Rust tests
cargo xtask validate-docs                          # 84 Requirement IDs
cargo xtask validate-manifests                     # 2 manifests
cargo xtask adapter-smoke
cargo xtask ci
corepack pnpm install --frozen-lockfile
corepack pnpm typecheck
corepack pnpm build                                # 26 modules
cargo check -p capyio-desktop
cargo build -p capyio-desktop
git diff --check
```

The Codex shell initially omitted both Cargo and Node from its process `PATH`.
The installed tools were found at `C:\Users\arthu\.cargo\bin` and
`C:\Program Files\nodejs`; the successful full CI run prepended those locations
to that process only. No tool was installed and no persistent system setting was
changed. The pinned frontend package manager resolved to pnpm 11.5.3 through
Corepack. The Tauri build emitted a non-fatal Windows linker informational
message while creating import-library artifacts and completed successfully.

Rust test distribution for the full workspace run:

| Area | Tests |
|---|---:|
| Adapter Host | 9 |
| Adapter SDK and manifest contract | 12 |
| Audio | 10 |
| Core | 12 |
| Protocol | 10 |
| Runtime | 4 |
| Testkit | 13 |
| **Total** | **70** |

## Changed files

Adapter process/control boundary:

- `crates/capyio-adapter-host/Cargo.toml`
- `crates/capyio-adapter-host/src/lib.rs`
- `crates/capyio-adapter-host/src/bin/adapter-smoke.rs`
- `crates/capyio-adapter-host/src/bin/host-fixture.rs`
- `crates/capyio-adapter-host/tests/host_protocol.rs`
- `crates/capyio-adapter-sdk/src/lib.rs`
- `crates/capyio-adapter-sdk/src/manifest.rs`
- `crates/capyio-adapter-sdk/src/rpc.rs`
- `crates/capyio-adapter-sdk/tests/manifest_contract.rs`
- `adapters/mock-source/Cargo.toml`
- `adapters/mock-source/src/lib.rs`
- `adapters/mock-source/adapter.json`
- `adapters/mock-sink/adapter.json`
- `protocol/schemas/adapter-manifest.schema.json`

Core/Runtime behavior:

- `crates/capyio-core/src/error.rs`
- `crates/capyio-core/src/lib.rs`
- `crates/capyio-core/src/route.rs`
- `crates/capyio-core/tests/route_model.rs`
- `crates/capyio-runtime/src/runtime.rs`
- `crates/capyio-testkit/src/lib.rs`

Validation, UI and merge gates:

- `scripts/validate_repository.py`
- `xtask/src/main.rs`
- `.github/workflows/core.yml`
- `.github/workflows/static.yml`
- `.github/workflows/ui.yml`
- `.github/workflows/tauri-smoke.yml`
- `apps/desktop/src/App.vue`
- `Cargo.lock`

Architecture and evidence:

- `docs/ADAPTER_MODEL.md`
- `docs/ARCHITECTURE.md`
- `docs/BACKLOG.md`
- `docs/BUILD_STATUS.md`
- `docs/DOMAIN_MODEL.md`
- `docs/GATE_0_3_REPORT.md`
- `docs/PROTOCOL.md`
- `docs/TESTING.md`
- `docs/REQUIREMENTS_TRACEABILITY.md`
- `docs/adr/0020-adapter-manifest-v2-mode-bindings.md`
- `docs/plans/completed/0004-capyio-foundation-hardening.md`
- `docs/FOUNDATION_HARDENING_REPORT.md`

No new third-party production package was added. The Mock source now declares
the already-present workspace Serde dependency, which accounts for the
`Cargo.lock` package-dependency update.

## Unresolved risks and non-claims

- GitHub-hosted Windows/Linux/macOS jobs have not run for this working tree;
  workflow configuration is not a hosted pass.
- Linux/macOS Sidecar child-process behavior has not been exercised locally.
- The Host is deliberately sequential. A poisoned Host requires a Supervisor
  to construct a new process; concurrent request/event demultiplexing is not
  implemented.
- Continuous payload transport, flow control, clock synchronization and real
  StandardPort data are absent.
- There is no SensorServer integration, Android app/APK, phone permission flow,
  real microphone/speaker path, Windows endpoint/driver, pairing, encryption,
  production signing, packaging or release evidence.
- No APK, driver or physical-device operation was performed.

## Follow-up boundary

The next implementation should begin as a new, reviewed task after this change
is committed and its hosted checks are observed. The backlog currently names
that product slice `CAPY-IMU-001`; it must start with recorded fixtures and keep
physical-phone actions separately authorized.
