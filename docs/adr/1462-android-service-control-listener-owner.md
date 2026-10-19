# ADR 1462: Android Service owns the control listener

- Status: Accepted lifecycle integration; discovery advertisement pending
- Date: 2026-09-18

## Context

ADR 1461 produced a socket-owning Rust listener but the unified Android app did
not create or retain it. Consequently no physical Windows process could reach
the listener and the shared UI would continue to report every control state as
disconnected even after a successful exchange.

## Decision

For exactly one active paired record, `AudioNodeService` recovers its typed
credential, creates the catalog Runtime, transfers the credential through a
bounded JNI byte array into a native listener bound to `0.0.0.0:0`, then clears
the Java plaintext array. A daemon Java worker polls the non-blocking acceptor
at 100 ms while idle. It retains the first successful `CatalogControlLink` in
the native listener registry; it does not repeatedly admit the same peer.

JNI registries are bounded to four listeners and four catalog Runtimes. Empty
polls release both locks immediately. An actual connection may hold the
listener and Runtime registries for the bounded five-second TLS/catalog timeout;
this serializes catalog replacement during first admission but performs no
real-time hardware callback work.

Service destruction first stops the worker, removes the listener, closes and
reconciles its exact Session, and only then destroys the catalog Runtime. The
shared UI projects a retained successful link as connected transport, active
catalog exchange and one authenticated connection. It does not label the
control socket itself as LAN discovery.

## Consequences

- The unified Android process now owns a real control listener independently of
  Activity lifetime.
- A successful authenticated catalog exchange is visible through the same UI
  schema used by Windows.
- The ephemeral port is not yet advertised, so automatic Windows discovery is
  still unavailable.
- Worker failure text remains host-internal; reconnect and retry policy remain
  pending.
