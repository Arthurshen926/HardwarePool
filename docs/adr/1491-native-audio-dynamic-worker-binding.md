# ADR 1491: Bounded dynamic native audio worker bindings

Status: accepted for integration groundwork only

Date: 2026-09-26

## Decision

Provide matching Rust and Android native audio binding values carrying caller
supplied Session, Route, Stream, epoch and finite grant deadline. Reject nil
identities, epochs outside 1..2147483647 and deadlines outside the JavaScript
safe integer range. Android configuration factories recheck expiry and preserve
all dynamic identities instead of substituting laboratory constants.

Rust construction additionally applies ADR 1490's live Route checks, requires
AdapterManaged and an exact supported PCM preset. Reuse requires the original
grant deadline, not an implicitly extended grant. The only supported presets
are 48 kHz S16LE 10 ms mono microphone and stereo speaker. Unsupported formats
are rejected rather than silently converted.

## Boundaries and remaining integration

These values are not authorization tokens, a network protocol or a complete
admission decision. Android cannot establish trusted live Runtime state from
these values alone. Callers must validate catalog direction, negotiated format,
current authorization and leases, obtain addresses from authenticated control,
and stop workers on revocation, expiry or disconnect.

No production ConfigLoader, service or control offer is rewired in this slice.
Existing laboratory starts still use their fixed identities. Native UDP still
has no authentication, encryption, integrity or replay protection. Control TLS
does not provide these properties to the separate UDP channel. Product UI must
not advertise automatic secure audio Route use on this basis.

No dependency, permission, driver, wire field or signing change is introduced.

ADR 1492 subsequently connects the Android microphone adapter's internal
handoff to a guarded sender. The Service/control offer remains unconnected;
the public product readiness boundary above is unchanged.

## Verification

Rust tests cover changed grants, deadlines, backend, epoch overflow, exact PCM
presets and actual loopback UDP delivery with freshly generated identities.
Android contract tests cover identity preservation into both worker configs,
nil identities, signed narrowing, deadline boundaries and invalid endpoints.
These tests do not prove physical audio or product-level Route integration.
