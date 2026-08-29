# ADR 0035: Share one direction-neutral audio media engine

Status: accepted

## Context

The completed remote-speaker path and the planned Android-to-Windows microphone
path both need bounded audio framing, format selection, queue policy, clock
recovery and diagnostics. Duplicating those mechanisms would make the two
directions drift, but forcing both workflows through one fixed format or
processing chain would make voice, media playback and lossless music compromise
each other.

Ports already own direction in the CapyIO domain model. A new global audio
provider/consumer role would duplicate that model and violate the symmetric
Node architecture.

## Decision

Use one transport- and platform-independent `capyio-audio` model for both
directions. It owns decoded formats, selected stream specifications, bounded
QoS policies, deterministic candidate negotiation, frame semantics, reordering,
clock estimates and common metrics. It does not open sockets, access hardware,
run codecs or choose a production transport.

Microphone and speaker remain independent Capabilities, Ports, Routes,
authorizations and lifecycles as required by ADR 0004. Platform capture/render
Adapters, Windows projection endpoints, concrete transports, codec
implementations and processing implementations remain outside `capyio-audio`.

Each Route selects a concrete `AudioStreamSpec`. Initial policy presets are:

- `VoiceInteractive`: low-latency mono voice with optional voice processing;
- `MediaBalanced`: ordinary stereo playback without voice processing;
- `MusicLossless`: lossless PCM playback without voice processing.

The presets are policy defaults, not wire protocols. A 96 kHz or multichannel
claim requires endpoint capability and physical-path evidence rather than only
a representable format.

Two opposite-direction Routes may later be associated in a duplex bundle for
an AEC reference. The association does not merge permission, failure or stop
state.

## Consequences

- The proven Speaker private wire and render ring can remain unchanged while
  mapping to the common selected specification and metrics.
- A MicYou compatibility Adapter can use the same model without making its
  private data plane a StandardPort.
- Voice processing can be enabled for a conference profile without altering
  lossless music playback.
- Concrete PCM/Opus/AOO/MicYou dependencies still require their own provenance,
  license and integration decisions.
- Production transport selection remains deferred under ADR 0006.

