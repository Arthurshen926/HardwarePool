# ADR 1494: Explicit native audio lab endpoint offers over authenticated control

Status: accepted for the control-link integration slice only

Date: 2026-09-28

## Decision and compatibility

Append RouteStatus.native_audio_data at field 3 and raise the envelope minor
version to 1.4. Existing fields retain their meaning and number. The new offer
contains a nonzero port, non-nil Stream UUID, endpoint side and explicit closed
preset/transport enums. Initially only mono 48 kHz S16LE / 10 ms and trusted-lab
UDP v1 are recognized. No implicit default transport, host address, credential
or audio payload is transmitted. Unknown or zero enums are errors.

The codec requires a Starting/Active AdapterManaged audio Route, finite safe
grant deadline and epoch in 1..2147483647. It rejects simultaneous touchpad and
audio offers. Clock freshness, selected format and leases remain host checks.

The control link exposes send_native_audio_data_offer and receives typed offers
on the existing mutually authenticated stream. The sender can advertise only
its own Source/Sink endpoint; the other endpoint must be the authenticated
peer. Outbound expired grants fail before sending. An inbound owner mismatch
or malformed RouteStatus closes the link. Exchanging offers does not mutate
Runtime, acquire a lease or start a hardware worker.

Old peers can ignore this append-only optional field. Absence of a matching
reciprocal offer must fail negotiation; no caller may silently substitute fixed
laboratory IDs or an unauthenticated discovery address. Existing touchpad and
plain RouteStatus behavior is retained.

## Remaining orchestration

Product Service/desktop loops do not yet initiate this exchange or consume it
to start microphone workers. They must retain owner consent and finite leases,
select matching catalog formats, match both offers' Route/Session/Stream/epoch,
bind actual sockets, and invoke ADR 1492/1493 workers transactionally. Denial,
timeout, disconnect and Stop must release both workers and local claims.
Active requires media evidence rather than a control acknowledgement.

No change to UDP authentication, encryption, integrity or replay defenses is
claimed. Explicit trusted-lab policy remains mandatory before using this
transport; a TLS control connection does not encrypt separate UDP datagrams.
No new dependency, permission, driver or APK change is introduced.

## Tests

Protocol tests cover both endpoint directions, absent offer compatibility,
conflicting offers, port/UUID/enum bounds, non-running and unbounded Routes,
unsupported backend/Profile and epoch bounds. Real loopback mTLS control tests
exchange sink and source offers with the same Stream, reject wrong-side local
advertisements and expired sends, and close after a peer forges the other
Node's endpoint. This is control metadata evidence, not audio delivery evidence.

Recorded verification: protocol 24 tests and control-link 16 tests passed;
Clippy all-targets with warnings denied passed for both crates. Desktop and
Android JNI host cargo checks passed, as did documentation validation and
diff whitespace checks. These are not an Android ARM64 build, installed APK,
physical media test or packaged release.
