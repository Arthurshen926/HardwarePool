# CapyIO Audio Frames Profile v1

> Profile: `capyio.audio.frames/1`
> Status: shared semantic/media baseline; Audio Share and MicYou compatibility
> Adapters map only preserved semantics without claiming StandardPort
> interoperability.

## Role and direction

The same Profile describes timestamped audio frames. Direction comes from Port:

- physical microphone: Source;
- physical speaker: Sink;
- Windows virtual microphone projection: Sink;
- Windows virtual render/system-mix capture: Source.

Microphone and speaker remain separate Capabilities/Ports. A duplex UI template
creates independent Routes and authorization; it is not a magic duplex stream.

## Baseline formats

- signed PCM little-endian 16-bit;
- 48 kHz;
- microphone baseline mono;
- speaker baseline stereo;
- explicit channel layout;
- initial 10 ms frame duration.

Additional PCM widths/codecs may be declared as format descriptors. A Route
selects an advertised mutual format; unsupported formats fail explicitly.

## Frame timing

Each continuous epoch identifies Stream/Route epoch, sequence number, monotonic
source timestamp, first-sample index, sample count and discontinuity. Reconnect
or format change creates a fresh epoch. Old-epoch frames are discarded.

## QoS

Audio can map to `Basic`, `Interactive` or `Measurement` intent plus
audio-specific descriptors such as media playback, voice interactive and raw
capture. QoS does not imply a transport or codec.

The first selected-stream contract exposes three bounded policy presets:

- `VoiceInteractive`: 48 kHz mono PCM baseline and low-latency voice policy;
- `MediaBalanced`: 48 kHz stereo PCM baseline for ordinary playback;
- `MusicLossless`: lossless PCM policy with voice processing disabled.

These are distinct complete candidates. The first negotiation revision selects
the first Source-preferred candidate also advertised exactly by the Sink. It
does not silently resample, transcode, enable processing or alter buffer policy.
Representing 96 kHz or discrete channels is not evidence that an endpoint or
physical Android path supports them.

## Processing

AEC, noise suppression, gain control and raw capture support/enabled state are
explicit negotiated metadata. A host reports actual accepted platform state,
not merely requested flags. Unsupported processing is not silently advertised.

Voice processing or raw capture may be requested only for a
`VoiceInteractive` candidate. Raw capture cannot be combined with AEC, noise
suppression or AGC. Media and lossless-music candidates reject these flags.

## Encoding boundary

The semantic contract can identify PCM or an explicitly configured Opus
candidate, but `capyio-audio` contains no codec implementation. A concrete
Adapter must record the codec implementation, version, license, packet bounds
and failure behavior. The current Audio Share compatibility transport accepts
only PCM and preserves its pinned private wire bytes.

ADR 0041 binds each active media channel to one Session, Route, Stream, positive
epoch and exact selected specification. `AudioMediaPacket` can contain PCM or
encoded payload without repeating or changing encoding per packet. PCM converts
losslessly to the decoded `AudioFrame`; encoded payload stays opaque until an
Adapter-owned codec decodes it. The packet is an in-process contract, not a
network byte layout and not evidence that Opus is implemented.

The reference `BoundedAudioPacketQueue` applies both packet-count and aggregate
payload-byte limits and rejects wrong Stream/epoch data. It is a worker-thread
boundary, not the lock-free platform callback ring.

ADR 0042 adds a typed backend declaration for media visibility, PCM/Opus
support, metadata fidelity and security. Full-packet StandardPort audio must
preserve the common contract exactly. The Audio Share compatibility backend is
PCM-payload-only and strips common packet metadata after validation. MicYou is
opaque to CapyIO: its private PCM/Opus capability and voice mapping do not prove
the exact codec, timing or packet metadata selected for a common stream.

## Common metrics

Adapters map observable counters into a common snapshot covering produced
blocks, unconsumed blocks, payload bytes, packets, loss/duplicate/late data,
queue underrun/overrun, transport errors, discontinuities and optional jitter,
buffer-fill and clock-drift estimates. An unavailable metric remains
unavailable; a default zero is not evidence that a private transport measured
zero loss.

## Receiver behavior

User-mode Audio Adapters own bounded reorder/jitter buffering, loss/duplicate
detection, underrun/overrun counters, clock estimation and resampling. Callback
threads do not perform network I/O, parsing, logging or unbounded allocation.

## Permissions and lifecycle

Microphone capture requires the visible platform permission/lifecycle state and
immediate revoke handling. Speaker playback follows platform focus/route rules.
Actual sample rate, channel and buffer parameters are reported after stream open.

## Volume

OS endpoint volume, route gain and physical device volume are separate concepts.
A Port advertises only controls it can actually apply. Remote global device
volume is never assumed.

## Errors

Typed errors distinguish unsupported format, permission denied/revoked, route
unavailable, busy device, focus denial, stream-open failure, processing
unavailable, buffer thresholds, clock recovery and transport discontinuity.
