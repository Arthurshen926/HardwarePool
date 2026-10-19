# ADR 1487: Android control I/O does not hold the catalog Runtime lock

- Status: Accepted for integrated-alpha lifecycle responsiveness
- Date: 2026-09-26

## Context

ADR 1462 allowed first TLS/catalog admission to hold both native registries.
The persistent listener subsequently also held its own registry while waiting
for a control message. Local catalog replacement marked a flag through that
same listener registry, so a Service lifecycle transition or WebView snapshot
could wait behind remote network I/O. The five-second socket timeout bounded
each I/O operation, but did not make this an acceptable UI locking boundary.

## Decision

The Android host separates transport authentication, Runtime admission and
catalog exchange. TLS/Hello and catalog sends/receives run without a catalog
Runtime guard. The Runtime is locked only for deterministic admission,
preparation, application and failure reconciliation. The accepted ordering
remains receive-then-send; the dialed ordering remains send-then-receive. Any
failed exchange closes and reconciles its exact admitted Session.

The existing listener owner still serializes accepted and dialed attempts and
retains at most one active authenticated peer. Active pairing, exact leaf
pinning, ordered Node IDs and a fresh Session are unchanged. No endpoint hint
can grant trust or resource authorization.

Each successful local catalog replacement advances a checked, monotonic
publication revision within the Runtime. The control owner remembers the
revision of the catalog actually prepared for transmission. On an existing
peer heartbeat it publishes when these revisions differ, advancing its own
revision only after a successful send. A replacement during network I/O
therefore remains pending for the next heartbeat. Failed replacements change
neither Runtime state nor revision. This supersedes ADR 1462's Runtime-lock
duration and the listener-mutex dirty flag; it adds no further global lock.

The Java listener caches the bound control port before starting its worker.
Read-only UI status thus does not need the listener registry while that worker
waits on a socket. The port remains valid only while that owner is live.

## Verification and limits

`cargo test -p capyio-android-jni` includes real pinned-TLS loopback cases that
delay each direction's first catalog. Concurrent Runtime replacement must
finish before the delayed peer is allowed to reply. The accepting path sends
the new revision; the dialing path preserves a revision written after its
initial publication as unsent. A malformed catalog still closes the admitted
Session, and the existing wrong-leaf/unpaired tests retain their rejection.
The Android ARM64 `cargo ndk` check covers the JNI registry integration.

These are hardware-free concurrency checks. Device responsiveness, background
behavior and shutdown timing still require the integrated APK regression.
The listener registry remains worker-owned across network I/O; explicit
teardown can still wait for that worker's bounded transport operation. This
slice does not implement multiple peers or automatic connection rendezvous.
