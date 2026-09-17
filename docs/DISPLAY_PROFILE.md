# CapyIO Display Profile

> Status: normative pre-alpha semantic contract for `capyio.display.frames/1`.

## Scope

`capyio.display.frames/1` carries decoded display content from exactly one
Source Port to one Sink Port. The primary Source is a Windows virtual monitor.
Windows chooses duplicate or extend in its display topology; that choice does
not change this Profile, encoder, transport or Android Sink. A system-picker
capture of an existing display is retained only as an explicit no-driver
fallback and uses a separate platform Adapter.

This Profile does not define Windows Graphics Capture, IddCx, Android Surface
rendering, H.264/H.265/AV1, Sunshine/Moonlight, a socket or a driver IPC. Frame
bytes never travel in Node Protobuf or Sidecar JSON-RPC.

## Relationship to Video

The Display crate reuses bounded rational frame-rate, pixel-format and
colorimetry value types from `capyio-video`. It does not reuse the Video Profile
identity. A `capyio.video.frames/1` camera/preview Port and a
`capyio.display.frames/1` Port are incompatible without an explicit Converter.

## Selected stream

`DisplayStreamSpec` is one exact complete candidate:

- positive dimensions no larger than 8192 pixels per axis;
- a positive reduced rational frame rate bounded by the Video value contract;
- packed `BGRA8` with full-range sRGB or even-dimension packed `NV12` with a
  limited-range BT preset;
- composited or hidden cursor semantics;
- positive target/maximum latency bounds;
- explicit permission to replace pending frames instead of accumulating an
  interactive backlog.

Packed size is exactly `width * height * 4` for BGRA8 or
`width * height * 3 / 2` for NV12 and cannot exceed 128 MiB. Negotiation selects
the first Source-preferred candidate also advertised identically by the Sink.
It never resizes, rotates, mirrors, changes color, encodes or rewrites QoS.

The canonical Core format descriptor is `packed-raw-display-v1` with exact
width, height, reduced frame rate, pixel format, colorimetry and cursor mode.

## Frame and retained-content semantics

Each `DisplayFrameDescriptor` carries `StreamId`, positive epoch, sequence,
source-monotonic timestamp, positive duration, exact payload length and flags.
Normal frames carry exactly one selected packed payload. `clear_surface` and
`end_of_stream` are distinct zero-payload intents and cannot be combined.

Disconnect/retry advances the Route/stream epoch. Older frames cannot become
current after retry. A Sink should clear retained content when instructed or
when its Route policy transitions to a privacy-sensitive terminal state.

## Latest-frame handoff

`LatestDisplayFrameSlot` holds at most one accepted frame. A newer frame
replaces an unconsumed older frame, increments an observable replacement count
and preserves sequence-gap evidence. Wrong streams, stale/future epochs,
duplicate/late frames and non-advancing epoch changes are rejected explicitly.

This capacity-one owned-memory helper is for deterministic user-mode handoff
and tests. It is not a network transport, codec queue or real-time driver ABI.

## Deterministic fixture

The BGRA fixture generates a reproducible synthetic pattern from coordinates
and sequence number. It reads no window, display, device or personal content and
proves no physical mirror, extended display, latency or quality behavior.

## Windows virtual-monitor handoff

Each authorized remote Display Sink Capability can project one primary Windows
IddCx virtual-monitor Source. Multiple projections use independent monitor
identities, swapchains, frame handoffs and Routes. Windows selects duplicate or
extend topology for each monitor; its swapchain path receives the resulting
pixels in either case.

The IDD accepts only bounded BGRA8 swapchain textures, copies through one
CPU-readable D3D11 staging texture on its worker thread, removes GPU row
padding, and publishes the newest frame into a fixed 64 MiB Broker-owned
mapping. The mapping has a 64-byte ABI header, generation binding and an
odd/even sequence lock. Its ACL permits the creating owner plus the Windows
service identities required by the UMDF host; another ordinary user cannot
open it. Closing the Broker invalidates the generation so the driver detaches.
No codec, network, pairing or reconnect policy executes in the driver. A
bounded topology coordinator controls monitor lifecycle and preserves foreign
display paths; it does not carry pixels or serialize per-monitor media work.

## Windows existing-display capture fallback

`capyio-windows-display` currently projects a system-picker-selected Windows Graphics
Capture item into this Profile. The first projection is exact BGRA8/sRGB only.
The Windows capture border remains required, and no programmatic HWND/HMONITOR
target constructor or borderless capability is exposed.

The free-threaded WGC callbacks only update bounded atomics. A caller-owned
worker drains to the newest available frame, copies its GPU texture through a
reusable D3D11 staging texture, removes row padding and hands one owned packed
frame to `LatestDisplayFrameSlot`. A target-size change emits clear content and
requires an exact new mode plus an advancing epoch before resuming. This CPU
readback is a correctness bridge, not a production codec or latency claim.

The desktop consent lab creates the picker on the Tauri owner window's UI
thread. Its backend owns a local Source-to-volatile-Sink Runtime Route and drops
each owned frame after updating bounded metadata. Raw pixels never cross the
Tauri command/DTO boundary. Resize clears content and advances the Route/source
epoch before exact reconfiguration.

This fallback cannot create a Windows monitor or implement the Windows
"duplicate these displays" topology. The primary virtual-monitor projection is
the separately isolated IddCx/IDD Adapter defined by ADR 0048.

## Encoded AdapterManaged seam

`capyio-display-transport` is separate from this decoded Profile. It describes
one private AVC Annex-B encoder-to-decoder seam with an exact decoded
`DisplayStreamSpec`, explicit AVC profile/level, bounded complete access units
and bounded Annex-B SPS/PPS configuration. Its explicit `CPD1` record functions
use one fixed complete-record layout shared with Android Java fixtures; the
bytes remain forbidden from the control plane and imply no socket, security or
public interoperability contract.

On Windows, `capyio-windows-display` now converts the IDD mapping's packed
BGRA8/sRGB snapshot to bounded BT.709 limited-range NV12 and submits it to the
in-box Media Foundation H.264 encoder MFT. The output is normalized to four-byte
Annex-B start codes. A transport-bound producer emits SPS/PPS configuration
before the first access unit and preserves the selected Stream ID and epoch.
The Session/Route binding is supplied by an authenticated outer transport; this
producer provides no socket, AEAD, peer authentication or Route authorization.

The encoded selection also validates AVC macroblocks per frame and macroblocks
per second against its declared level. The first phone-aspect virtual-monitor
candidate is 1980x912 at 30 fps: it has the approved V2419A viewport's exact
165:76 aspect ratio and requires at least AVC Level 4. The Windows Media
Foundation path selects a bounded access-unit buffer bucket from the decoded
NV12 size (2, 4, 8 or 16 MiB); its transform-reported minimum must fit that
authenticated selection before streaming begins. A Windows topology change
still requires a new exact Route selection and advancing epoch rather than an
implicit decoder resize.

`capyio-android-display` applies the lifecycle expected by a future Android
Surface decoder: configure with codec-specific data, wait for a key frame on
startup, flush and resubmit configuration after a sequence gap or explicit
discontinuity, wait for another key frame, and require stop/configure with a
newer epoch for a format change. It consumes each borrowed access unit
synchronously and retains zero encoded payload bytes.

The Rust crates contain the Windows encoder seam and platform-neutral decoder
lifecycle, but no socket, Android SDK/JNI bridge or physical-device evidence.
The separately compiled Android Library implements the decoder and
private-record parser; its minimal application module owns a full-screen Surface
and decoder worker. Neither opens a network path or declares a permission or
foreground service. None of these additions changes the normative
`capyio.display.frames/1` payload into an encoded Profile.
