# ADR 1428: Reject ambiguous display provisioning and close failed sockets

Status: accepted for unified Android integration

Date: 2026-09-17

## Context and decision

The imported Android loader used ordinary Properties loading, which silently
overwrites duplicate keys and ignores extra fields. This does not meet the
closed configuration contract described by the display integration. Before
extending provisioning, use explicit complete field sets for transport, CPD1
and GameStream files. Reject duplicate decoded keys (including escaped aliases),
unknown/missing keys, blank/overlong/control-character values and oversized
input. Standard Properties comments and escaping remain supported. Previously
ignored extra fields now cause explicit failure; no silent compatibility fallback
is added. Semantic binding and TLS credential verification remain mandatory at
their existing boundaries.

The shared receiver preflight additionally checks the selected properties
structure before enabling Open. It reads no key contents and establishes no
network connection. File presence and structural acceptance remain distinct
from valid credentials, successful TLS authentication and rendered frames.

The TLS connector now transfers socket ownership only after configuration,
connect, handshake and protocol/ALPN checks succeed. Failed initialization closes
the socket, preserving the primary exception and retaining a cleanup exception
as suppressed. Successful initialization leaves ownership with the existing
caller. The CPD1 loader wipes its temporary caller-owned key array after copying
into the configuration; GameStream validates its Expected binding before reading
key material. This is not a guarantee of erasing provider or JVM-internal copies.

## Evidence and limits

The existing display prerequisite gate now includes 13 structural parsing cases
and five socket ownership cases. Fake unconnected sockets verify transfer,
exception/security/error cleanup and original-error preservation. No TLS peer,
certificate issuance, Android permission change, APK installation or display
capture is performed by these tests. Unified offline capyioCheck additionally
compiles the actual loader/connector, runs Lint and verifies native packaging.

These corrections do not implement ordinary-user pairing, certificate enrollment
or physical media acceptance. They add no dependency or network protocol.
