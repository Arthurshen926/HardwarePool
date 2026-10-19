# LINUX-A05 — Finite-batch kernel queue acceptance hardening

Status: accepted on exact implementation commit `4e8c249` (hosted Linux only)
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
No local socket tests ran. These local checks did not establish socket acceptance;
the subsequent hosted attempts are recorded below.

## Hosted fixture correction

Run 39295014378 on `b75ddba19b330dfe50239ed1737425da3592e714` failed in
setup: after one filler returned WouldBlock, a fresh sender could still enqueue.
That is sender-buffer pressure, not demonstrated receiver queue saturation or a
production defect. The revised setup retains bounded multiple senders and keeps
filling until an independently fresh sender is blocked before any successful
write. It fails explicitly if aggregate limits prevent establishing that state.
No local sockets were attempted. Historical acceptance of `07b068e` does not
cover this fixture correction.

## Exact-head hosted acceptance (2026-10-19)

Implementation `4e8c24907532daf781a8760ab12283edd67d542f` passed
[run 39295525829, job 118206892623](https://github.com/Arthurshen926/HardwarePool/actions/runs/39295525829/job/118206892623)
on GitHub-hosted Ubuntu 24.04.5 with Rust 1.97.1. Formatting, strict Clippy and
mock binary build passed. All 40 focused tests passed (14 host, 14 mock library,
12 node subprocess), with zero failed, ignored or filtered tests. The full logs
verify all 18 required successful outcomes, not only their test-list presence,
including `batch_saturated_kernel_queue_bounds_source_failure_and_cleans_endpoint`.

This establishes bounded Source failure after independently verified target
queue pressure and the test fixture's socket/directory cleanup. The two-second
elapsed assertion is not a watchdog; the workflow's 240-second timeout and
10-second kill-after bound catch hangs. This adds no subprocess lifecycle proof,
continuous-streaming, hardware or production-networking acceptance. The original
cloud socket restriction remains unchanged. A later documentation-only evidence
commit is not the tested implementation SHA.
