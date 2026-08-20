# Plan 0001 — Bootstrap validation

Status: complete

## Objective

Turn the generated bootstrap archive into the first reproducibly building repository without expanding product scope.

## Inputs

- all root specifications and `AGENTS.md`;
- source skeleton in `crates/`, `apps/`, `protocol/` and `xtask/`;
- a Windows development host with online dependency access.

## Milestones

### M1 — Toolchain and lockfiles

Status: **complete**.

- install Rust 1.97.1, Node 24 LTS and pnpm 11.5.3;
- run `cargo xtask doctor`;
- run `pnpm install`;
- generate and commit `Cargo.lock` and `pnpm-lock.yaml`;
- record actual Tauri/Gradle compatibility issues if any.

### M2 — Rust build

Local status: **complete**. Formatting, check, Clippy, all 28 tests and all four
CLI flows pass on Windows.

- run formatter;
- fix Core/Runtime/Protocol compile errors without changing architecture semantics;
- pass all Rust tests;
- run all CLI flows and inspect their deterministic snapshot/protocol/audio
  output. The originally referenced `protocol/examples/runtime-snapshot.json`
  does not exist; the repository contains `ui-snapshot.json`, so no false file
  comparison was recorded.

### M3 — UI build

Local status: **complete**. Browser Mock and Tauri Demo were both exercised;
stopping microphone left speaker active.

- pass Vue typecheck and Vite build;
- run browser Mock mode;
- run Tauri desktop dev mode on Windows;
- verify independent capability controls.

### M4 — CI

Status: **complete**.

- create GitHub repository;
- push with explicit review;
- run Core and UI workflows;
- fix cross-platform-only errors;
- make required checks branch-protection candidates.

Evidence: initial commit `d1936d3`; hosted CI repair `4a658d5`; Rust core,
Shared UI and Static repository checks all passed on the repair commit.

## Exit criteria

- all M1–M4 steps have retained command output;
- `docs/BUILD_STATUS.md` reflects actual results;
- no real driver or microphone permission work has been added;
- Gate 0 in `ROADMAP.md` can be marked closed.

## Risks

- dependency versions may expose API drift because archive creation was offline;
- Tauri Linux CI may require adjusted system packages;
- prost-generated enum naming may require small conversion fixes;
- Windows host may be ARM64 rather than x64; record it before choosing driver target.

## Windows validation evidence

- Host target: Windows build 26200, `x86_64-pc-windows-msvc`.
- Lockfiles: generated and dependency installation completed with pnpm 11.5.3.
- Dependency repairs: ADR 0007 and ADR 0008.
- `cargo xtask ci`: pass.
- `cargo check -p hardwarepool-gui --all-targets`: pass.
- `pnpm tauri build --debug --no-bundle`: pass.
- `pnpm tauri dev --no-watch`: window launched as `TAURI_DEMO` in desktop-user
  context. The restricted Codex process cannot perform Tauri setup directly and
  returns Windows error 5; retrying outside that sandbox succeeds.
- Browser and Tauri interactive checks both preserve independent microphone and
  speaker state.
