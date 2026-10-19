# ADR 1484: Windows persistent paired inbound control listener

- Status: Accepted for integrated-alpha manual peer connection
- Date: 2026-09-19

## Context

Android can now initiate a pinned control TLS connection (ADR 1483), but the
Windows inbound gate in ADR 1481 closes after one diagnostic exchange.

## Decision

The Windows desktop Node owns one explicitly started inbound listener scoped
to one active paired peer and one chosen local numeric IP/port. It loads the
same current-user DPAPI credential and pinned peer leaf as outbound control,
requires the expected ordered Node binding with a fresh Session, and rejects
failed handshakes without terminating the listener. An accepted peer exchanges
both catalogs with the process-owned Runtime and enters the existing live
`ControlLinkController`; the UI sees the same connection, remote catalog and
Route path regardless of which peer dialed. The worker responds to peer
heartbeat requests while awaiting its own acknowledgement. Stopping the
listener prevents new connections but does not silently stop an established
Route. App shutdown stops the listener before its links.

The accepted TCP peer address is only a data-channel endpoint hint after TLS
pinning; it never issues trust or grants resource use. Local listener startup
and outbound dialing now arbitrate through the native listener owner: a live
outgoing link prevents listener startup, and an active listener prevents an
outgoing dial to that same peer. This avoids a local one-slot race, but is not
a distributed simultaneous-dial or automatic rendezvous protocol. No
automatic listener startup, certificate rotation, firewall mutation or
driver action is introduced.

The Settings page reads the actual listener owner on refresh, rather than
using a WebView-only boolean; navigation or WebView reload cannot falsely
present an active listener as stopped. The readback contains only the chosen
peer Node ID and local endpoint, never credential material.

## Verification and remaining work

Desktop Cargo tests and shared UI typecheck cover this slice. A synthetic
loopback test uses newly issued dual-role identities, reverse TLS, catalog
exchange and three rounds of bidirectional heartbeats. After explicit renewal,
the Y700 physically dialed the Windows listener; authentication, catalog
exchange and sustained TCP connection succeeded, and Windows Remote Resources
showed its capabilities. Automatic rendezvous, distributed simultaneous-dial
arbitration and the physical touchpad Route remain open.
