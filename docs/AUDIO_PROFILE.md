# CapyIO Audio Frames Profile v1

> Profile: `capyio.audio.frames/1`
> Status: semantic baseline; no real Audio Adapter is connected.

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

## Processing

AEC, noise suppression, gain control and raw capture support/enabled state are
explicit negotiated metadata. A host reports actual accepted platform state,
not merely requested flags. Unsupported processing is not silently advertised.

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
