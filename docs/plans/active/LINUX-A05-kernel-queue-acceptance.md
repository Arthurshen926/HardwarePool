# LINUX-A05 — Finite-batch kernel queue acceptance hardening

Status: implementation prepared; hosted acceptance pending
Created: 2026-10-19
Requirements: `FR-PORT-004`, `NFR-STAB-002`

## Scope

Test the existing ADR 1517 Source failure contract when its Unix datagram Sink
queue is saturated. No runtime changes, dependencies, continuous streaming,
hardware, IP networking, configuration changes or local socket execution.

## Acceptance

- Nonblocking setup discovers saturation within 4096 aggregate send attempts,
  32 retained sender sockets and a two-second setup budget; it never assumes
  or changes a kernel queue setting.
- A fresh nonblocking sender must fail on its first write to confirm target
  pressure independently of sender-buffer exhaustion. Earlier sender sockets
  remain live until the Source test finishes.
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

## Hosted fixture correction

Run 39295014378 on `b75ddba19b330dfe50239ed1737425da3592e714` failed in
setup: after one filler returned WouldBlock, a fresh sender could still enqueue.
That is sender-buffer pressure, not demonstrated receiver queue saturation or a
production defect. The revised setup retains bounded multiple senders and keeps
filling until an independently fresh sender is blocked before any successful
write. It fails explicitly if aggregate limits prevent establishing that state.
The revised fixture still requires exact-head hosted execution; no local sockets
were attempted and prior hosted acceptance must not be attributed to this fix.
