# CapyIO Foundation Baseline Report

Date: 2026-08-23 (Asia/Shanghai)  
Baseline commit: `d33d585`  
Baseline branch: `feat/hp-android-001-shell`  
Migration branch: `codex/capyio-foundation`  
Status: Gate 0 complete

## Scope

This report records the unmodified HardwarePool bootstrap immediately before
the CapyIO v0.3 foundation migration. It is evidence for buildability only. It
does not claim that a real network, Android hardware path, Windows projection or
Adapter sidecar exists.

## Repository summary

```text
crates/
  hardwarepool-core       deterministic audio-centric domain model
  hardwarepool-audio      audio frame/reorder/drift primitives
  hardwarepool-protocol   Protobuf v1 conversion and envelope codec
  hardwarepool-runtime    peer/session/binding orchestration
  hardwarepool-testkit    deterministic Android/Windows fixtures
apps/
  hardwarepool-node       headless demo CLI
  gui                     Vue + Tauri deterministic control surface
protocol/
  proto/hardwarepool/v1   HardwarePool control schemas
platform/                 Android/Windows placeholders
drivers/                  Windows audio design placeholder only
xtask/                    unified development commands
scripts/                  repository validation and inventory helpers
docs/                     v0.2 requirements, architecture, Profiles and ADRs
```

The baseline workspace contains eight Rust packages including `xtask` and the
Tauri shell. The active model is audio-centric: nodes advertise global roles,
Capabilities carry local/stream roles and supported Projections, and Sessions
own per-Capability Bindings.

## Architecture mismatch motivating the migration

The v0.2 documents and implementation fix Android as provider and Windows as
consumer. The owner-provided CapyIO v0.3 direction instead requires symmetric
nodes, direction on typed Ports, Source→Sink Routes, and existing vertical
projects behind Adapter boundaries. This is a deliberate pre-alpha breaking
migration and is captured in the new ADR set; the old requirements remain in
Git history and `docs/references/` where applicable.

## Environment inventory

| Tool | Baseline observation |
|---|---|
| Git | 2.53.0.windows.3 |
| rustc | 1.97.1 (8bab26f4f, 2026-07-14) |
| Cargo | 1.97.1 |
| Node.js | 24.19.0 |
| Corepack | 0.35.0 |
| Python | 3.12.10 |
| pnpm available on Codex PATH | 11.19.0 |
| repository `packageManager` pin | 11.5.3 |
| Java | OpenJDK 25.0.2 when Android Studio JBR is added to session PATH |
| ADB | not present on the audited session PATH |
| MSBuild / WinDbg | not present on the audited session PATH |

The persisted Windows environment was not modified during this audit. The
current Codex process did not inherit the installed Rust/Node/JBR paths, so the
successful rerun used this process-local prefix:

```powershell
$env:Path = 'C:\Users\arthu\.cargo\bin;C:\Program Files\nodejs;C:\Program Files\Android\Android Studio\jbr\bin;' + $env:Path
```

The first `cargo xtask doctor` therefore reported Node and Corepack missing.
After the session PATH was corrected, all required bootstrap tools passed. That
initial failure is retained here because it is relevant to reproducibility.

## Baseline command evidence

| Command | Result | Key evidence |
|---|---|---|
| `python scripts/validate_repository.py` | PASS | structural validation passed in 5.89 s |
| `cargo xtask doctor` (inherited PATH) | FAIL | Node and Corepack not visible to the Codex process |
| `cargo xtask doctor` (session PATH) | PASS | required tools/files present; 0.95 s after build |
| `cargo fmt --all -- --check` | PASS | no formatting differences; 0.32 s |
| `cargo check --workspace` | PASS | all workspace packages including Tauri; 0.79 s cached rerun |
| `cargo test --workspace` | PASS | 34 tests passed, 0 failed; 4.70 s cached rerun |
| `pnpm typecheck` | PASS | `vue-tsc --noEmit`; 4.54 s |
| `pnpm build` | PASS | Vite built 26 modules; 1.30 s bundle stage |
| `pnpm install` | SKIPPED | lockfile and populated `node_modules` already present; no install required |

`cargo test --workspace` emitted one non-fatal MSVC linker informational
message while creating the Tauri import library. No Rust test failed.

## Baseline implementation status

Implemented and verified:

- deterministic Core validation and audio Binding lifecycle;
- bounded host-operation completion/cancellation seam;
- Protobuf envelope and node round trips;
- audio frame validation, reorder buffering and drift estimation;
- browser Mock and Tauri demo UI builds;
- hosted CI had already passed on the baseline history.

Not implemented or not verified:

- symmetric Capability/Port/Route domain model;
- CapyIO naming or `capyio.v1` protocol;
- Adapter SDK, manifest, sidecar or supervisor;
- real networking, audio, camera, IMU, recording or system Projection;
- Android application build/install or physical-device behavior;
- Windows driver build/install or isolated-VM validation;
- production pairing, identity, authorization or encryption.

## Gate 0 conclusion

The bootstrap is a healthy, reproducible starting point. The migration can
proceed incrementally without first repairing failing application tests. Tool
PATH normalization and the pnpm 11.5.3/11.19.0 discrepancy must be addressed by
the Gate 1 toolchain documentation and final validation.

