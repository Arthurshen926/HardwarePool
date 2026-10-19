# ADR 1436: Mutually authenticated Node control transport

Status: Accepted foundation; pairing lifecycle pending

## Context

The protocol and admitted control host can exchange catalogs and owner lease
decisions, but previously no generic Node control connection provided
confidentiality, peer authentication, framing or downgrade resistance. Display
has a separate media-specific mTLS transport whose records and Route binding
must not become the generic control protocol.

## Decision

Add `capyio-control-tls`, an independent user-mode TCP/TLS boundary. It requires
TLS 1.3, mutual certificate verification, the dedicated `capyio-control/1`
ALPN, disabled early data/server tickets and caller-provided trust roots for the
already-paired peer. After TLS authentication, both sides exchange an exact
fixed record containing Session ID, server Node ID and client Node ID. Only the
expected record is acknowledged.

Control Envelopes use a four-byte big-endian length followed by one complete
validated Protobuf Envelope. Empty frames and frames above 1 MiB fail before
allocation. Connect, handshake and subsequent reads/writes use caller-selected
deadlines within 100 milliseconds through 30 seconds. One authenticated test
connects the channel to `capyio-control-host` and proves an owner-authoritative
Port lease decision traverses the complete boundary.

This reuses workspace-pinned rustls 0.23.43 (Apache-2.0 OR ISC OR MIT) with its
ring provider. Test-only rcgen 0.14.5 (MIT OR Apache-2.0) creates ephemeral
certificates. Alternatives considered were reusing the display media transport
(rejected as incorrect coupling), plaintext trusted-LAN control (rejected for
production), and bespoke cryptography (rejected as unsafe and unnecessary).

## Consequences

Catalog and lease Envelopes can now cross a real mutually authenticated channel
without importing sockets or cryptography into Core, Runtime or drivers. The
transport does not enroll, persist, rotate or revoke certificates, does not
choose a discovered peer, and does not schedule multiple connections or
reconnect. Platform hosts must supply a securely stored paired identity and
must not treat a broad public trust store as CapyIO authorization.
