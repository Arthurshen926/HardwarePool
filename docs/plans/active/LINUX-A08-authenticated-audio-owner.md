# LINUX-A08 — Accepted authenticated audio owner

Status: implemented; review and hosted TLS/IPC acceptance pending
Created: 2026-10-20
Requirements: `FR-SESSION-002`, `FR-ROUTE-004`, `FR-ROUTE-005`, `NFR-SEC-001`, `NFR-SEC-005`

## Scope

ADR 1520 consumes an already-authenticated accepted TLS channel and an existing
locally authorized single-Route Runtime. Receive reciprocal offers from that
channel, bind actual peer IP, validate exact offers/current local lease, and drive
ADR 1519's worker. No listener, credential provisioning, default-host change,
new grant, physical device operation or secure-UDP claim.

## Acceptance

- Require exact expected paired Session/local/peer identity, not arbitrary Ready.
- Reject wrong bindings/offer/preset/endpoint and expired/missing local authority
  before media opens. Never expose a method accepting fabricated peer status.
- Active follows real worker readiness; failures/Stop/Drop close owned media and
  the dedicated Session, releasing leases without touching other Sessions.
- Actual pinned-TLS/UDP/child fixtures must run on hosted CI; local socket tests
  are forbidden. Keep bounded terminal-timeout and caller-driven polling limits
  explicit; this is not an autonomous control/media scheduler.

## Next

Compose Linux pairing/catalog/local-consent provisioning and a scheduled control
reader/media owner, without blocking playback on control I/O. Preserve the
explicit trusted-lab restriction until secure Android-compatible media exists.
No physical compatibility claim without separately authorized device tests.

## Local verification (2026-10-20)

Pinned Rust 1.97.1: 23 socket-free audio tests passed; 11 IPC tests were explicitly
filtered and remain unaccepted locally. Audio/TLS/link/host/node all-target test
compilation and strict Clippy passed, as did workspace formatting and repository
structural validation (88 requirements). Python suite: 25 tests successful, one
existing Windows-only skip. Hosted workflow retains previous guards, adds owner
and absolute-deadline guards, and includes full TLS/link suites. No local socket,
credential provisioning, actual PipeWire or Android-device action occurred.
