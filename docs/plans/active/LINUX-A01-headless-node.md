# LINUX-A01 — Linux headless Node management

Status: implemented; targeted validation passed
Owner: cloud development
Created: 2026-10-19
Requirements: `FR-NODE-001`, `FR-NODE-002`, `FR-NODE-005`, `FR-PLAT-003`, `NFR-STAB-002`

## Objective

Run an actual Linux NodeRuntime-owning process with stable caller-supplied identity,
bounded local management, deterministic rejection and clean explicit/EOF exit.

## Read first

Root AGENTS.md, the eight normative foundation documents, apps/capyio-node/AGENTS.md,
and ADR 1514.

## Scope and constraints

One Linux platform crate and a thin CLI entrypoint. No hardware, network, credentials,
new external dependencies, service installation or publication. Catalog stays empty.
Mock Route/session lifecycle is a separate next slice and is not acceptance here.

## Acceptance criteria

1. Multiple real subprocess requests retain a Linux identity and the same process.
2. Snapshot/status return structured JSON without stdout logging or phantom devices.
3. Malformed/schema/version/unknown-command requests reject safely; request and
   response memory/output are bounded.
4. EOF and shutdown exit cleanly; oversized unfinished requests cannot force a
   newline drain. Test failures have timeouts and child cleanup.
5. Documentation distinguishes this host from Linux physical support.

## Required validation

- cargo test -p capyio-linux-host -p capyio-node
- cargo check -p capyio-linux-host -p capyio-node
- cargo clippy -p capyio-linux-host -p capyio-node --all-targets -- -D warnings
- cargo fmt --all -- --check
- python3 scripts/validate_repository.py
- python3 -m unittest discover -s scripts -p 'test_*.py'

## Dependency and safety record

Only existing workspace dependencies are reused. No privileged/device operation,
installation, deployment, commit or push is included. Subprocesses are locally
built executables with synthetic requests only.

## Completion record

Linux x86_64 cloud run, 2026-10-19, pinned Rust 1.97.1:

- Targeted Rust tests: 6 unit and 4 real subprocess tests passed, including array-shaped shutdown
  rejection and subsequent valid-request recovery.
- Repository structural validation passed (88 requirements).
- Python portable suite: 25 tests passed, with one Windows junction test skipped.
- Targeted check/Clippy (warnings denied) and workspace format: passed.

No full workspace/Windows/Android, physical I/O, mock Route, network, Tauri packaging,
service-installation or production-security acceptance is claimed.
