# ADR 1461: Android single-peer control listener

- Status: Accepted Rust listener owner; Service/JNI lifecycle pending
- Date: 2026-09-18

## Context

ADR 1460 composed an accepted authenticated channel with Android Runtime, but
did not bind a socket or select trust material. The current TLS server identity
pins one exact client leaf and one peer CA, while the pairing directory can
represent multiple active peers. Silently choosing among them would make the
trust decision ambiguous.

## Decision

`AndroidControlListenerV1` owns a caller-created TCP listener and constructs the
fresh-Session mTLS server only when the canonical directory contains exactly
one active peer. Zero active peers reports not configured; more than one fails
closed until a deliberate listener-selection or multi-peer TLS policy exists.

The constructor binds the local and peer Node IDs, exact peer certificate
fingerprint and decoded typed credential bundle into
`AuthenticatedControlServer`. It exposes the actual bound address and an
`accept_and_exchange` operation that delegates initial catalog processing to
ADR 1460. The listener never advertises itself and never starts a Route.

Binding remains caller-owned so the Android Service can select an ephemeral
port, retain the native owner and only then publish that port through trusted
discovery. This ADR does not yet add that JNI/Service lifecycle.

## Consequences

- Android now has a tested real socket-to-Runtime server composition rather
  than only an abstract accepted-channel helper.
- A multi-peer Android Node remains intentionally unsupported at the listener
  boundary even though its directory and UI count remain bounded to 64.
- Reconnect of an already admitted peer, disconnect observation and ongoing
  control-message dispatch remain separate work.
