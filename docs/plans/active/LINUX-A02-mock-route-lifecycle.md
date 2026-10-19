# LINUX-A02 — Linux mock subprocess Route lifecycle

Status: implemented; targeted validation passed, awaiting independent review
Owner: cloud development
Created: 2026-10-19
Requirements: `FR-ADAPTER-003`, `FR-ROUTE-001`, `FR-ROUTE-004`, `FR-DIAG-002`, `NFR-STAB-002`

## Scope

ADR 1515: opt-in known repository mock children, real returned catalog registration,
one synthetic Session/Route, staged start/stop, matching child completion, rollback
and cleanup. Default host stays empty. No data transfer, hardware, network, real
trust, leases, OS settings, deployment, commit or push is included.

## Acceptance

- Repository source/sink processes initialize and publish test-only catalogs.
- Explicit mock lifecycle produces Runtime state/events, matching epochs, idempotent
  start/stop and new epochs on restart.
- Partial failure and wrong completion binding fail closed and close both children.
- Exit is reconciled at status/snapshot or repeated Start; no background watchdog claim.
- EOF, oversized input and shutdown exit, with fixed object inventory and bounded
  saturated snapshot. The normal empty host and strict parser remain unchanged.
- Docs distinguish simulated lifecycle from absent Source-to-Sink data transfer.

## Validation

```text
cargo build -p capyio-mock-source -p capyio-mock-sink
cargo test -p capyio-linux-host -p capyio-node
cargo check -p capyio-linux-host -p capyio-node
cargo clippy -p capyio-linux-host -p capyio-node --all-targets -- -D warnings
cargo fmt --all -- --check
python3 scripts/validate_repository.py
python3 -m unittest discover -s scripts -p 'test_*.py'
```

The core CI and `cargo xtask test` build the mock binaries before tests. Tests do
not silently skip missing binaries. The `/proc` descendant inventory is unavailable
in this cloud environment; no external process-tree inventory claim is made.


## Completion evidence

Linux x86_64 cloud, pinned Rust 1.97.1, 2026-10-19:

- 11 host unit tests and 8 Linux subprocess tests passed.
- Targeted check and Clippy with warnings denied passed, including xtask.
- Workspace formatting and repository structural validation passed.
- Python portable suite: 25 tests, one existing Windows-only skip, passed.

No full workspace, Windows/Android, source-to-sink sample transport, physical I/O,
external descendant PID inventory, hostile-child timing or production-security
acceptance is claimed. No commit/push/deployment performed in this slice.
