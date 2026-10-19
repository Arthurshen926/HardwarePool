# LINUX-A05 — Finite-batch kernel queue acceptance hardening

Status: implementation prepared; hosted acceptance pending
Created: 2026-10-19
Requirements: `FR-PORT-004`, `NFR-STAB-002`

## Scope

Test the existing ADR 1517 Source failure contract when its Unix datagram Sink
queue is saturated. No runtime changes, dependencies, continuous streaming,
hardware, IP networking, configuration changes or local socket execution.

## Acceptance

- Nonblocking setup discovers saturation within 4096 sends and a two-second
  setup budget; it never assumes or changes a kernel queue setting.
- A separate fresh nonblocking sender confirms receiver-side saturation.
- The real four-record Source Start fails with WouldBlock/TimedOut within the
  existing two-second scheduling tolerance for its 500 ms I/O budget.
- All test sockets close before the endpoint owner removes its socket/directory.
- The dedicated hosted workflow requires this regression by exact test name.
- Setup that cannot establish saturation fails explicitly, never silently skips.

## Verification boundaries

Compile and run non-socket checks locally only. Actual saturation acceptance
requires the authorized hosted Linux workflow on the exact resulting commit.
The prior 39-test/17-guard pass belongs only to implementation `07b068e`; the
new regression is not covered by that historical evidence. This is a Transfer
boundary test, not additional subprocess cleanup or general backpressure proof.

## Local evidence (2026-10-19)

Pinned Rust 1.97.1: targeted all-target check, strict Clippy, test compilation and
workspace formatting passed. The three socket-free batch parser/queue tests and
14 host unit tests passed. Repository validation passed (88 requirements); the
Python suite ran 25 tests successfully with one existing Windows-only skip.
No local socket tests ran. Hosted acceptance for the new regression remains
pending; no commit, push or workflow execution was performed in this slice.
