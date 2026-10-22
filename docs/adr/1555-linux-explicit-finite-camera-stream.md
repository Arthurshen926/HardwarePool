# ADR 1555: Explicit finite canonical camera streaming on Linux

Status: experimental; synthetic hosted graph acceptance required. Production
Android advertisement and capture configuration do not implement this profile.

## Additive compatibility boundary

The exact empty-parameter format `cavc-avc-finite-720p30-v1` is an opt-in secure
camera profile under the existing video-frames profile and AdapterManaged route.
It is not a widened interpretation of `cavc-avc-access-unit-v1`. An explicit Linux
manifest field `"stream_profile":"finite_720p30_v1"` chooses a Sink advertising
only the new descriptor. Absence retains the legacy format and 120-frame/32-MiB
work limits. Null, unknown profiles and unsupported Sources fail; no fallback,
remote catalog rewriting or implicit upgrade occurs.

Secure camera offers recognize the new exact descriptor; plaintext camera offers
remain legacy-only. CAVC records, control envelopes, offer fields, TLS ALPN and
route-binding bytes do not change. A selected descriptor is pinned in the secure
owner and checked with the existing immutable Route, Session, ports, Stream,
epoch, grant and lease identity. Incoming authenticated status cannot change it.
This is a new explicitly selected capability format, not new envelope semantics.

These are deterministic local profile ceilings, not claims that identical numeric
per-session work limits were exchanged on the wire. Existing authenticated
resource grants and Source/Sink leases remain the authority boundary.

## Work limits do not extend time authority

The canonical media stays 1280 by 720, 30 fps, zero rotation, Annex B AVC with the
existing SPS/PPS, color, reference/B-frame and decoder-output restrictions.
The finite decoder normalizes its remaining lifetime down to whole milliseconds,
rejects zero and values above 60 seconds, and derives immutable work ceilings:

- Access units: ceil(milliseconds × 30 / 1000), at most 1,800.
- Complete CAVC bytes: min(32 MiB, 2 MiB/second × duration + 4 MiB).
- Arrival rate: local monotonic 30-AU/second bucket with three-AU burst.
- Encoded-byte arrival rate: local monotonic 2-MiB/second bucket with 4-MiB burst.

Ceil applies only to a work count. It never extends an exact deadline or grant.
Negotiation and handshake consume the original finite lifetime; decoder startup
uses the remaining budget, independently bounded by current original authority.
Native relative timers begin later during child startup; exact original grant
expiry remains the responsibility of the outer owner supervision and guards,
not a claim that every helper timer equals that absolute grant deadline.
PTS jumps, discontinuities, retries or subsequent polls cannot refill/reset the
budget. The byte burst counts complete records, so an AU at the allocation-size
ceiling may exceed the smaller admissible complete-record burst.

Rust and the native decoder independently enforce the new profile, including
checks before native payload reads/decoding. Native profile arguments are a closed
choice, not arbitrary peer-supplied limits. Legacy helper arguments and legacy
payload accounting remain unchanged; finite accounting charges complete-CAVC
sizes (CONFIG payload + 84 bytes, AU/EOS payload + 56 bytes).

The original 4-MiB AU, 64-KiB CSD, 2-second bounded decoder IPC, 512-MiB native
address-space and 10-CPU-second ceilings remain. The 32-MiB total or CPU ceiling
can end a high-work session before its requested duration. Sixty seconds is a
maximum authority window, not promised continuous delivery or hardware capacity.

## Memory and lifecycle remain bounded

This removes neither queue limits nor backpressure. The receiver retains bounded
records, decodes one AU at a time, and passes fixed-size NV12 frames through the
existing one-slot queue, writer-owned frame and fixed native/PipeWire buffers.
It does not retain the whole stream. Native latest-frame publication may lose
frames. The secure owner remains fail-closed on the first full Rust projection
queue: DroppedBackpressure terminates the session rather than permitting continued
secure streaming. Actual queued/dropped counters are not consumer-delivery or
OS-camera-usage evidence.
The console retains Starting-only semantics and consumer_delivery_verified=false.

Stop, control EOF, revoke, exact lease expiry, malformed input, excess work/rate
and helper failure terminate and reconcile owned children. There is no renewal,
reconnect, automatic restart or longer-lived grant. Finite status adds the real
dropped counter; legacy status framing remains unchanged.

## What is and is not implemented

The actual Linux Node and console can accept an explicit finite manifest and
match its fresh approval to the new profile and duration-derived maxima.
`prepare-camera` export remains legacy in this slice; it does not silently select
or write the new profile. Read-only catalog output exposes actual format IDs,
while its default camera compatibility/export hint remains legacy-oriented.

This ADR left the Android catalog, Java Camera2 settings and production sender
unchanged. [ADR 1556](1556-android-rust-finite-camera-sender.md) subsequently adds
finite admission to the actual Rust sender, while Android catalog advertisement
and Java capture remain legacy-only. Supporting a phone still requires a
separate profile-aware canonical capture/configuration and sender-budget path,
then explicit advertisement and separately authorized physical-device evidence.
An old Android Source is expected to reject this new-profile session, not work
through fallback. Synthetic acceptance is not a phone compatibility claim.

## Acceptance

Pure/native tests preserve legacy frame 121 rejection and codec constraints while
checking finite frame/byte/rate boundaries, overflow, malformed profiles and
non-resetting counters. Protocol/guard tests cover secure-only format support,
extra parameters, mismatch and mutation without widening plaintext offers.

The new hosted actual-Node graph path uses an owned private fake Secret Service,
synthetic Source with existing Android resource-authority policy and production
TLS transport, not the production Android camera sender. Its sustained scenario
submits 300 paced frames over approximately ten seconds under a twelve-second
approval ceiling, allowing existing negotiation/lease headroom. An independent
consumer checks complete NV12 pixels and sequence/PTS, records actual delivered
count/span and requires observations beyond frame 120, at least five seconds of
observed span and a late sequence of at least 240. It does not equate 300 submitted
or queued frames with 300 delivered frames. Bounded supervised draining never
extends the grant. Separate scenarios cover Stop, revoke, EOF, expiry, paced
backpressure, malicious arrival rate, oversized input and incompatible peers.
The backpressure scenario first establishes sustained independent pixel evidence,
then pauses only its pidfd-pinned owned projection helper. Valid paced frames fill
the bounded queue; the existing secure-owner rejection must terminate and reap
both children while the original Source authority is still live. It does not
expect a dropped-frame event or successful continuation after queue exhaustion.
All existing legacy camera, audio, touchpad, pairing and platform gates remain.
No real camera, user keyring or user device is used by these tests.
