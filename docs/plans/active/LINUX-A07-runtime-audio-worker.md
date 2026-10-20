# LINUX-A07 — Runtime-driven Linux audio worker

Status: implemented; local verification complete; independent review and hosted IPC pending
Created: 2026-10-20
Requirements: `FR-ROUTE-004`, `FR-ROUTE-005`, `FR-SESSION-002`, `NFR-SEC-005`, `NFR-STAB-002`

## Bounded scope

ADR 1519 adds a real UDP/PipeWire worker for already admitted dynamic Runtime
Routes. Reuse existing binding and lease checks. Keep the default host and
static lab CLI unchanged. No untrusted control API, new authentication claim,
physical device operation, installation, permission change or publication.

## Acceptance

- Validate exact dynamic contract and finite local Sink lease before any open.
- Revalidate after opening and after every bounded receive before writing PCM.
- Stop/failure closes owned resources, releases matching leases, and never
  mutates a restarted epoch. Retain honest readiness and Offline diagnostics.
- Tests cover expiry, lost lease, changed grant, closed Session, clock regression,
  partial open, write/cleanup failure, lifetime and stale owner isolation.
- Hosted CI must execute actual dynamic UDP-to-child lifecycle and all previous
  lab/IPC tests. Local socket execution is excluded; no silent skip is acceptance.

## Next boundary

Integrate a Linux authenticated control owner with negotiated catalogs and
explicit local consent, then add secure Android-compatible media transport.
This library is not callable through `linux-host` management yet. A separately
authorized physical PipeWire/Android test remains required; virtual microphone
projection is still separate. Caller-driven polling is not a watchdog.

## Local verification (2026-10-20)

Pinned Rust 1.97.1: 20 socket-free Linux audio tests passed; five IPC tests were
explicitly filtered, not accepted. All focused audio/host/node test targets
compiled. Strict Clippy for those three packages and workspace formatting passed.
Repository structural validation passed (88 requirement IDs); the Python suite
ran 25 tests successfully with one existing Windows-only skip. No local sockets,
PipeWire, Android device, actual capture or playback were executed. The dedicated
hosted workflow now retains all previous guards plus 12 Route-worker guards;
acceptance on the eventual exact published head is still pending.
