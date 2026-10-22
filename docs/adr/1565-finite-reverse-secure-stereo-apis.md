# ADR 1565: Finite reverse secure stereo APIs

Status: Candidate

## Scope and compatibility

Add reusable Linux capture Source and Android Rust Sink APIs for explicitly
selected secure stereo audio. This is not Android AudioTrack integration and
not a Node/console launch mode. Android speaker publication and Java/JNI live
playback remain unchanged and must not advertise the new profile yet.

The first profile is finite, at most 60 seconds, with no renewal, reconnect or
plaintext fallback. Legacy UDP, mono TLS 60-second and mono TLS 300-second
profiles retain their codecs, descriptors, defaults and limits. There is no
change to the global Port-lease ceiling or any permission/service setting.

## Exact transport and atomic records

The empty-parameter descriptor is `pcm-s16le-48000-stereo-tls13-v1` with
`capyio.audio.frames/1`, AdapterManaged and Interactive selected QoS. The fixed
PCM specification is `AudioStreamSpec::media_balanced()`: 48 kHz, signed 16-bit
little-endian interleaved left/right stereo, 480 sample frames per 10 ms block.
The secure admission path does not use UDP loss concealment or reordering.

`SecureAudioTransport::Tls13MtlsStereoV1` uses wire value 2. Value 1 remains mono
only. A separate TLS channel uses ALPN `capyio-audio-stereo/1` and binding magic
`CPYAST01`; the existing mono channel remains exactly 1064 bytes per record.
Protocol minor 8 is retained for this additive explicit opt-in. Unsupported
peers reject the descriptor/discriminator; absence or failure never selects UDP.

The native-LAN codec's 1200-byte datagram ceiling is **not** increased. Each new
TLS record contains exactly 2128 bytes, a pair of existing canonical CPYA
fragments:

- Fragment 0: 1200 encoded bytes, payload 1096 bytes, offset 0.
- Fragment 1: 928 encoded bytes, payload 824 bytes, offset 1096.
- Both: count 2, total PCM 1920 bytes, sample count 480, identical complete
  Session/Route/Stream/epoch/sequence/sample-index/timestamp metadata.

Both fixed slices are validated before PCM is published or the ordered timeline
advances. Unknown/reserved flags, discontinuity, mismatched or swapped fragments,
truncation, replay, gaps and checked-arithmetic overflow are terminal. No UDP
reassembler or partially admitted frame is used. Sequence starts at zero and
advances by one; sample index advances by 480 and PTS by 10000 microseconds.

The rate budget counts whole records, not fragments: 100 records/second,
four-record burst and at most 6000 records for the 60-second profile. PCM buffers
and worker handoffs are fixed-size; total session work is not accumulated audio
in memory. Counters/PTS cannot extend authority.

## Consent, leases and current peer identity

Pairing, catalog publication and TLS readiness never authorize capture/playback.
The API owners require independently granted local consent and a fresh Prepared
Route. A preexisting local lease for that generation is rejected rather than
reacquired. From explicitly consented Prepared state, the Linux Source sends one
Sink Acquire and the Android Sink sends one Source Acquire. The two lease
exchanges may cross. Each endpoint answers only its initial authenticated
request, pins the resulting local claim, and consumes the exact matching remote
response once. Once both claims are fixed, the Source performs its local Start
transition and sends authenticated Start; the Sink consumes that Start before
media setup. Re-Acquire, replayed response or mutation of a pinned claim is
terminal.

No media offer, TLS handshake or helper activation occurs before Start and both
claims are established. The original consent monotonic deadline is anchored
before negotiation and intersected with grant and both lease expiries. Full
Route, selected format/spec/QoS, Session, peer, generation and claims remain
immutable, except the explicitly handled Prepared-to-Starting transition.
No background lease refresh or generation recycling is introduced.

The Android reusable owner has its own authenticated `CatalogControlLink` and
current explicitly supplied paired server identity. Media listener role does
not imply control-server role. It does not reuse a stale camera/touchpad identity
cache or enter the existing JNI dispatcher, which currently rejects unsolicited
PortLeaseResponse messages. Production JNI integration must separately add its
bounded pending-request lifecycle and exact current-session identity admission.

## Worker and sink boundaries

After authenticated TLS binding, the reusable Sink requires one explicit local
PCM-ready enable before issuing its first read permit. Enable is one-shot and
rejected before binding, after termination/expiry, or when repeated. While
waiting, the owner continues control/Stop and original-authority supervision;
no media read permit or Sink write is issued. This readiness wait is bounded by
the original finite authority, not by a media operation that has already begun.
The first actual record and all later records retain the unchanged 250 ms
operation budget. Readiness never refreshes consent, grants or either lease.
The graph fixture uses this boundary after verifying the owned Source helper
and planned exact link endpoints, then establishes and independently validates
the two channel links. Both owners keep being polled throughout preparation.

Blocking TLS work is independently cancellable and bounded by the earlier of
its operation deadline and original authority. Partial progress does not restart
an operation deadline. Stop, control loss, grant/lease revocation, profile or
catalog mutation and expiry close admission and owned transport; failed work is
not retried on another transport.

A portable sink accepts nonblocking aligned partial PCM writes. Fresh authority
is checked before and after each write, with one nonresetting frame deadline.
Zero progress, slow trickle, invalid byte counts and backpressure cannot grow
queues or sustain an expired generation. Owner shutdown joins workers and
cleans up only its owned resources. This is bounded application design, not a
hard arbitrary OS scheduling/driver guarantee.

Routes remain Starting and counters describe actual transport or sink acceptance,
not physical audibility. This API slice neither creates an AudioTrack nor claims
that an Android application consumed or played audio.

## Required evidence and later integration

Pure tests cover exact fragments/format, overflow, timeline, rate/work bounds,
mono/UDP compatibility and profile substitution. Mandatory hosted tests exercise
actual pinned TLS, wrong peer/ALPN/binding, partial/slow/truncated records, malformed
second fragments, replay, cancellation, backpressure, original expiry and both
local/remote authority changes. The mandatory wire names must actually execute;
compilation/listing and the inherited one-shot listener no-op are not wire
evidence.

An isolated PipeWire graph supplies independently generated, differing left/right
tones, silence and a later tone to the real Linux capture API and real Android
Rust receiver. Independent decoded PCM analysis must prove channel order and
content, not merely packet counters. Stop/revoke/EOF/expiry evidence records the
terminal observation before cleanup and verifies owned helper/graph removal.
This is synthetic API proof, not actual Android AudioTrack or phone proof.

Follow-on slices must add a separately selected Android speaker profile, bounded
JNI PCM ownership/acknowledgement and AudioTrack lifecycle, then actual paired-store
Node/console capture and virtual-speaker paths. They must preserve legacy defaults,
fixed memory, current-generation consent and the original finite deadlines.
