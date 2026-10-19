# ADR 1473: Route-bound touchpad TLS data connection

Status: accepted

Date: 2026-09-18

## Context

The active user request is a complete Android/Windows touchpad workflow. ADR
1472 provides explicit owner consent, but the existing ADB lab receiver uses
fixed identities and cannot be exposed on the LAN. A logical Active Route is
not data readiness. The existing pinned mTLS credentials can authenticate a
separate connection without issuing a second device identity.

## Decision

Add a separate, bounded touchpad data transport beside the existing control
transport, reusing its reviewed rustls identity builders. Its distinct ALPN is
`capyio-touchpad/1`; control Envelopes cannot carry input records. Both sides
pin the exact paired certificate and require TLS 1.3 without early data.

The connection binds the exact Session, Source/Sink PortRefs, Route, fresh
Stream, epoch and finite authorization expiry. The platform derives this
binding from its current authorized Runtime Route, never from untrusted Hello
bytes. Reconnection requires a new authorized binding. Stop closes the owned
socket, including blocked reads; all framing/I/O failures are terminal.
Private CPTR records retain their existing AdapterManaged semantics and are
length-delimited with a 176-byte maximum. Each host must recheck Runtime
authorization and leases before opening its platform Sink and during delivery.

No Windows driver installation, ACL change, pairing bypass or plaintext LAN
fallback is introduced. Physical input still requires the independently
installed projection and permitted Windows token. Control v1.3 adds a bounded
port/Stream offer on RouteStatus. Android's existing foreground Service owns
the listener and dynamic record session; Windows registers a distinct native
AdapterManaged Sink and owns its data worker. Both remain Starting until the
initial Data ACK. Pending consent is polled on the existing control owner and
can be cancelled, without a second user click or a new proposal after denial.
First-pair UI and two-ended initiation remain separate integration work, not
implied by a transport test.

Physical follow-up fixes the Windows local lease conversion: Runtime takes an
absolute Unix expiry, while wire lease requests take TTLs. Stop is sent before
closing data; a correlated release denial is treated as already cleaned only
after the exact peer Route reports Stopped. Android records explicit cancellation
until its worker closes and completes only the matching lifecycle generation.

## Dependencies and compatibility

No new third-party dependency or version is added. The private transport uses
the existing rustls/ring, UUID and Core dependencies in `capyio-control-tls`;
the module is separate from control framing. Desktop/JNI reuse the existing
workspace protocol/Adapter crates; the Adapter's optional `tls` feature reuses
`capyio-control-tls`. Test certificates reuse the already-pinned rcgen release.
The optional v1.3 endpoint field preserves old message decoding, but real
touchpad startup fails closed against an older peer without an endpoint offer.
Reuse avoids duplicating certificate verification. A later generic TLS crate
extraction is preferable if more data Adapters need this seam.

## Acceptance

Hardware-free tests must prove mutual certificate pinning, wrong application
protocol rejection, exact Route/Stream/epoch binding, bounded framing, terminal
partial reads, authorization expiry and prompt explicit shutdown. Passing those
tests does not establish physical cursor movement or a finished product flow.
