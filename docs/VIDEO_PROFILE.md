# CapyIO Video Profile

> Status: normative pre-alpha semantic contract for `capyio.video.frames/1`.

## Scope

`capyio.video.frames/1` describes direction-neutral, decoded, packed raw video
between one Source Port and one Sink Port. The owning Capability and Adapter
determine whether the Source is a phone camera, a fixture, a desktop capture or
another producer, and whether the Sink is a Panel, Recorder, Converter or
system Projection.

The Profile does not define Camera2, Media Foundation, RTSP/RTP, FFmpeg,
DirectShow, a socket, a codec bitstream or a Windows virtual-camera lifecycle.
Continuous frame bytes never travel in Node control envelopes or Sidecar
JSON-RPC.

## Selected stream

`VideoStreamSpec` is one complete candidate:

- positive bounded width and height;
- a positive reduced rational frame rate, bounded to 480 fps;
- packed `NV12` or packed `BGRA8` pixels;
- one closed colorimetry preset;
- one bounded QoS policy.

`NV12` requires even dimensions and a limited-range BT.601/709/2020 preset.
`BGRA8` requires full-range sRGB. Packed `NV12` is exactly
`width * height * 3 / 2` bytes and packed `BGRA8` is exactly
`width * height * 4` bytes. A frame and selected stream cannot exceed the
128 MiB bootstrap payload bound.

Frames are upright and unmirrored. Decode, resize, rotate, mirror and color
conversion are explicit Adapter or Converter work. Negotiation selects the
first complete Source-preferred candidate also advertised identically by the
Sink; it never performs an implicit conversion.

The canonical Core format descriptor ID is `packed-raw-video-v1`, with exact
width, height, reduced frame rate, pixel format and colorimetry parameters.

## Frame metadata

`VideoFrameDescriptor` identifies one payload on the selected data plane with:

- `StreamId`, positive epoch and sequence;
- source-monotonic timestamp and positive duration;
- separately carried payload length;
- discontinuity and end-of-stream flags.

The descriptor contains no frame bytes. A normal frame has the exact packed
payload length selected by `VideoStreamSpec`; an end-of-stream descriptor has
zero payload bytes. A new Route/reconnect epoch invalidates every older frame.
Gaps and discontinuities remain observable rather than being repaired by
rewriting sequence or timestamps.

## Camera capability metadata

`CameraDescriptor` augments one Core Camera Capability with cross-platform
facing, a fixed 0/90/180/270-degree sensor orientation, stream candidates and
closed zoom/torch capability descriptors. Core remains the owner of stable
Capability ID and display name.

Platform camera IDs, Android logical/physical relations, hardware level,
encoder inventory and concurrent-camera combinations belong to the bounded
Camera Adapter inventory DTO. They do not create a second public Core catalog.

## Encoded Adapter-managed paths

H.264/H.265 are deliberately not StandardPort v1 formats. A
VCamdroid-compatible RTSP/H.264 slice may retain its reviewed private data plane
behind matched `AdapterManaged` Ports. It cannot claim arbitrary
`capyio.video.frames/1` interoperability until access-unit framing, codec
profile/level, parameter sets, timestamps and decode output are specified and
tested through an explicit Converter boundary. The first private parser-only
record contract is specified in `AVC_ADAPTER_WIRE.md`; it is not a public video
Profile or a selected transport.

The Windows C5 lab converts guard-accepted Annex-B H.264 into bounded packed
NV12 with the inbox Media Foundation decoder. Its output remains inside the
private Adapter executable and is discarded after validation checksums; it does
not yet publish or negotiate `capyio.video.frames/1`, and therefore does not
change the interoperability statement above.

## Serialization and metrics

Rust Serde derives support bounded fixtures and diagnostic experiments; they
are not a public wire format. A concrete transport still requires framing,
authentication, replay/epoch binding and its own parser bounds.

`VideoMetrics` uses optional monotonic counters. `None` means the owning
Adapter cannot observe a metric; `Some(0)` is an observed zero.
