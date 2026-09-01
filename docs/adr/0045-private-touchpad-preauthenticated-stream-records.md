# ADR 0045: Frame private touchpad records inside a pre-authenticated reliable stream

Status: accepted

## Context

CAPY-PTP-002P through 002S establish transactional packet delivery, Runtime-
owned admission, a concrete in-process host channel and real Windows native
submission. A future Android-to-Windows connection still needs bounded record
boundaries and an acknowledgement contract before a socket implementation can
replace the local channel.

ADR 0044 deliberately keeps the 152-byte touchpad packet private and requires a
separate transport decision for authentication, replay defense, epoch binding,
MTU and framing. Selecting a complete public CapyDataPlane transport remains
premature, but leaving byte-stream boundaries implicit would make partial reads,
binding substitution and acknowledgement ambiguity impossible to test.

## Decision

Add a private v1 record codec to `capyio-remote-touchpad-adapter` for use only
inside an already mutually authenticated, encrypted, reliable and ordered byte
stream supplied by the trusted Node composition layer.

Every record begins with a fixed 24-byte little-endian header containing:

- magic `CPTR` and version 1;
- one closed record kind: Hello, Data, Ack or Close;
- bounded flags and a zero reserved byte;
- Route/stream epoch and packet sequence.

Exact record sizes are:

- Hello: 160 bytes, binding Route, Session, Source Port, Sink Port, epoch and
  optional authorization expiry;
- Data: 24-byte record header plus one complete 32..=152-byte private packet,
  with an exact maximum of 176 bytes;
- Ack: 24 bytes, echoing the exact epoch and sequence only after receiver-side
  acceptance and Sink processing; and
- Close: 24 bytes with the bound epoch and canonical zero sequence.

Outer Data epoch/sequence must equal the embedded packet fields. Unknown flags,
reserved data, lengths, versions, kinds and binding bytes fail before packet
delivery. An absent authorization expiry uses a cleared flag and canonical zero
expiry field.

An Ack loss or malformed/mismatched Ack is delivery-unknown. The sender must not
retry the packet in the same epoch because the receiver may already have
injected it; reconnect requires fail-closed cancellation and a later Route
epoch. A future I/O channel must configure finite connect/read/write deadlines
and must not commit transactional Source state before the exact Ack.

## Security boundary

The record codec performs no key exchange, authentication, encryption,
certificate validation, socket I/O, replay-window persistence or peer discovery.
Hello is binding confirmation protected by the surrounding authenticated
channel, not an authentication mechanism. Using these records over plaintext or
with an unverified peer is forbidden outside a separately scoped insecure lab.

## Consequences

- Stream record boundaries and maximum memory are explicit before network code.
- Route/Session/endpoint substitution can be rejected before Data acceptance.
- ACK ambiguity preserves the existing no-duplicate-gesture policy.
- The codec remains private `AdapterManaged` framing and makes no arbitrary
  StandardPort interoperability claim.
- A later production transport ADR must still select identity, pairing,
  authenticated encryption, key rotation and replay/downgrade protection.

## Alternatives considered

- Send raw `CPTP` packets directly over TCP: rejected because Route binding,
  record kind and acknowledgement semantics would remain implicit.
- UDP now: rejected because loss, reordering, MTU and acknowledgement policy
  would expand this slice before live Android timing evidence.
- Protobuf/JSON-RPC: rejected because continuous touchpad frames do not belong on
  the control path and variable diagnostic encoding is not the live data layout.
- Treat Hello as authentication: rejected because unprotected identity bytes do
  not prove possession of peer credentials.
