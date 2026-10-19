# LINUX-A04 — Fixed ordered mock batch

Status: implemented; socket-free validation passed; batch IPC unverified
Owner: cloud development
Created: 2026-10-19
Requirements: `FR-PORT-004`, `FR-ROUTE-004`, `FR-PROTO-004`, `NFR-STAB-002`

## Objective and scope

Implement ADR 1517's explicit four-record Unix-local fixture with exact ordering,
bounded queue rejection, complete aggregate receipt and existing owner cleanup.
Preserve ADR 1516 one-record behavior. No continuous stream, background workers,
hardware, networking, credentials, installation or new dependencies.

## Acceptance

- `--mock-batch` requires mock routes and conflicts with single-record payload.
- Exactly four consumed records with sequences 0..3 are required before Active.
- Binding, timing, corruption, replay, reordering, gaps and partial batches reject.
- Queue full does not evict accepted records or advance validation state.
- A single 500 ms budget covers each participant's complete batch I/O.
- Repeated Start does not resend; Stop clears receipt/path; restart changes Stream
  and epoch; EOF, shutdown, oversized input and failure release owned IPC.
- Existing eight single-record CI guards remain, with mandatory batch guards added.

## Validation and safety

Run formatting, targeted check/Clippy, repository validation and socket-free unit
tests locally. Compile all targets without executing socket tests. Run actual IPC
and subprocess acceptance only on the already authorized experimental GitHub CI
branch. Do not retry local IPC or escalate execution. No commit/push is part of
this implementation assignment. Retain a full patch including new files.

## Completion

Pending implementation, independent review and exact-head hosted CI. Queue-full
unit tests are not evidence of general transport flow control. Sequential Start
means Stop waits for the bounded transfer operation, not mid-transfer cancellation.

## Local implementation evidence (2026-10-19)

Targeted all-target check and Clippy with warnings denied passed, as did format,
repository validation (88 requirement IDs), two Adapter manifests and all-target
test compilation. Three batch parser/queue tests, existing single-record parser
and endpoint checks, 14 host unit tests and the CLI batch opt-in/conflict test
passed without socket execution. The independently calculated aggregate golden
is 84 token bytes and `fnv1a64:d5953109b0ac8ffd`.

All 17 mandatory CI guard names are present, retaining the original eight. A
CI-only paced partial-sender regression uses one joined, finite test helper
thread to distinguish the 500 ms total budget from renewed per-record timeouts;
production remains thread-free. Actual batch IPC, timeout and subprocess cleanup
acceptance remains pending independent review and exact-head hosted CI.
