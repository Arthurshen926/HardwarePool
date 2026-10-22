# ADR 1559: Finite pinned TLS mono audio

Status: implementation and isolated synthetic acceptance; no physical audio-device evidence.

## Decision and compatibility

Add an explicit AdapterManaged mono audio data path using the existing Rust
TLS 1.3 implementation and paired certificate pins. This is not an encryption
flag on the current UDP adapter. Existing trusted-lab UDP, stereo capture,
virtual endpoints, Android AudioRecord/AudioTrack integration and Windows
audio behavior remain separate. There is no automatic downgrade or reconnect.

The first slice exposes reusable Android Rust Source and Linux playback-owner
APIs. Its hosted fixture supplies synthetic PCM through the actual sender,
TLS connection and Linux owner into an owned PipeWire playback process. It does
not expose secure audio through the production console, start a microphone,
advertise Android secure-capture capability, or establish phone interoperability.
Future application integration must explicitly publish and select the new
format and preserve these authority and transport checks.

`RouteStatus.secure_audio_data`, field 7, is a separate receiver-only offer with
a nonzero listening port, Stream UUID and closed TLS transport discriminator.
It is mutually exclusive with other data offers. The exact selected format is
`pcm-s16le-48000-mono-tls13-v1`, with empty parameters and Interactive QoS, under
`capyio.audio.frames/1`. An ordinary UDP offer requires its exact legacy format;
it cannot be attached to a secure selection. Unknown/missing/mixed offers do
not permit fallback. An older implementation may ignore an unknown optional
field, but that does not supply the required secure selection or data channel.

The existing Android legacy audio handoff gains an explicit schema/transport
discriminator. Secure, missing or unknown transport cannot enter its
`DatagramSocket` preparation path. No private key is copied into Java, a JSON
DTO, an argument, an environment variable or a log.

## Transport and packet contract

The dedicated data connection uses `capyio-audio/1` ALPN and `CPYAUD01` binding
magic. Existing rustls/ring TLS 1.3, chain validation and exact paired leaf pins
are reused, without a new cipher, key-export protocol or crypto dependency.
The existing full Route binding/ACK binds the control Session and ordered Node
identities, Route, Source/Sink PortRefs, Stream, epoch and grant expiry. Linux
listens and Android dials in this first slice; socket role is not resource
direction or consent. The owner separately freezes the exact format, QoS,
resource and original local lease because these are not all in the TLS binding.

Each TLS record has a bounded u16 length followed by exactly 1,064 bytes: the
existing 104-byte canonical CPYA header and 960 mono S16LE PCM bytes. Each packet
contains exactly 480 samples at 48 kHz, representing 10 ms. No fragmented,
stereo, reserved-field, discontinuity or alternate-encoding interpretation is
accepted. Sequence starts at zero and advances by one; sample index starts at
zero and advances by 480; source timestamp advances by exactly 10,000 us from
its initial value. Overflow, duplicate, regression and gap are terminal. TCP
does not reuse the UDP reorder/reassembly policy. A length is rejected before
payload allocation; a partial or malformed frame cannot resume another stream.

The same pure canonical admission is applied independently by sender and
receiver. Each also has a monotonic 100-packet/s token budget with a four-packet
burst and a total ceiling of ceil(lifetime / 10 ms), at most 6,000 packets.
The work ceiling never grants time: exact original deadlines still stop I/O.
Neither PTS nor sequence can refresh a token budget, lease or generation.

## Consent, lifetime and ownership

Pairing and successful TLS are authentication, not resource permission. Both
endpoints require the exact live authorized Route and original local Port
lease. The Linux owner also requires an authenticated, matching Source lease
response and explicit local playback target. The Source uses the existing
Android resource-consent policy. No helper opens before consent, both leases
and the secure data binding ACK. A Route remains Starting; writing PCM is not
proof of audibility or application consumption.

The new API is one-shot and at most 60 seconds, further shortened by the
original grant and both leases. Its monotonic anchor precedes negotiation and
handshake. The authenticated offer receipt is consumed once; another secure
generation needs a fresh control connection. A changed lease, epoch, selection,
Session or generation is terminal, not renewal. Existing legacy duration limits
are unchanged. This is not long-session support.

Control supervision and network operations do not run in an audio callback or
under a long-held runtime registry lock. Queues and PCM staging are bounded;
network and helper stalls terminate rather than accumulate an unbounded
backlog. Individual I/O operations have at most a 250 ms budget, further
clamped by original authority. Partial progress does not reset that deadline.
The owner rechecks authority immediately before helper writes. Stop,
revocation, control EOF and lease loss interrupt owned transport and initiate
worker/helper cleanup. This is not a guarantee of cancelling already-buffered
audio at an exact physical sample boundary.
The reusable owner is caller-driven: its host must regularly poll it and invoke
Stop/Drop on termination. Transport operations have their own bounded permits,
but helper cleanup still requires host scheduling; this API is not an unattended
service or a hard real-time cleanup guarantee.
The reusable Source sender likewise does not own a control-loop thread. Its host
must reconcile the authenticated control link, provide fresh runtime snapshots,
and invoke the independent Stop handle on consent withdrawal, control loss or
generation change, including while a data write is blocked. This slice does not
claim that an unchanging caller-provided snapshot can discover such events.

TLS/TCP deliberately trades implementation reuse for head-of-line blocking:
loss can delay later audio. The bounded burst/stall policy may terminate a
jittery connection; it is not a general low-latency or loss-concealment promise.
Successful writes do not prove delivery timing. Encryption does not conceal
endpoint addresses, traffic sizes or timing, authenticate speaker acoustics,
or prevent an authorized malicious peer from sending unwanted PCM.

## Validation and dependency boundary

Pure tests cover exact offer/format/QoS, legacy separation, packet bounds,
timeline/overflow and monotonic budget behavior. Hosted wire tests exercise
actual paired TLS, binding/identity rejection, one-shot admission, malformed
records, deadlines and cancellation. The private PipeWire fixture uses
synthetic identities, explicit grants and independent monitor readback of
ordered tone/silence/tone, plus denial and lifecycle cleanup cases. It neither
uses a user's audio server nor opens a microphone or physical sound device.
Passing software tests does not establish physical Android audio support.

The implementation reuses internal `capyio-audio`, `capyio-native-audio-lan`,
`capyio-control-tls`, protocol and runtime crates. It adds no external production
dependency. TLS is preferred over introducing DTLS or QUIC in this bounded
slice because the existing pinned lifecycle and cancellation implementation
already has acceptance evidence. Alternative transports require separate
congestion, loss, MTU, replay, scheduling and platform validation.
