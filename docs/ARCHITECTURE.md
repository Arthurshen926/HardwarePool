# CapyIO Architecture

> Version: 0.4-pre-alpha
> Status: normative for the CapyIO foundation

## 1. Architecture objective

CapyIO uses a modular-monolith Node Runtime with selectively isolated Adapters.
It unifies node identity, catalogs, Port/Route semantics, lifecycle, permissions
and diagnostics without requiring every hardware class to share one data plane
or one system-Projection mechanism.

## 2. Principles

1. **Symmetric Nodes** — no global provider/consumer role.
2. **Direction on Ports** — Source produces, Sink consumes, Control receives
   commands.
3. **Small Core** — Node, AdapterInstance, Capability, Port, Route, Session and
   Problem are the stable object families.
4. **Replaceable mechanisms** — Profile semantics are separate from transport,
   codecs, platform APIs and projections.
5. **Adapters at trust/failure boundaries** — external projects and risky
   mechanisms do not leak into Core.
6. **Control plane first** — common control and diagnostics; data planes may
   remain Adapter-owned.
7. **Best-effort system Projection** — API/Panel/protocol/Recorder fallbacks are
   valid where transparent virtual devices are unavailable.
8. **Headless lifecycle** — UI controls Runtime but does not own it.

## 3. System structure

```text
CapyIO UI
  Quick Actions / Workspace / Built-in Panels
                         |
                  Local Management API
                         |
CapyIO Node Runtime (one logical runtime per device)
  Portable Core / Catalog / Routes / Sessions / Problems
  Adapter Registry / Adapter Supervisor / Platform Host
              |                         |
      In-process Adapter          Sidecar / External Service
              |                         |
     platform hardware APIs       existing projects/helpers
              +-------------+-----------+
                            |
                         Data planes
  CapyIO binary / local IPC / UDP / TCP / RTSP / WebRTC / USB-IP / ROS
```

The list of data planes is an integration boundary, not a current feature claim.

## 4. Layer ownership

### Portable Core

- typed identifiers;
- Node and Adapter catalogs;
- Capability/Port validation;
- Route compatibility and state machines;
- Session trust/control state;
- structured Problems.

Core contains no I/O, async runtime, environment access, platform SDK, Tauri,
Protobuf, codecs, drivers or concrete network transport.

### Runtime

- owns the local catalog and peer catalogs;
- atomically registers Adapter instances with their initial Capability catalog;
- reconciles catalog replacements against persisted Routes, invalidating only
  Routes whose endpoint contract is removed or changed;
- sequences commands, operation completions and events;
- owns Route/Adapter lifecycle orchestration;
- applies bounded retention;
- exposes snapshots to UI/CLI/management API.

Asynchronous Adapter callbacks complete Route work through staged Runtime
commands (`authorize`, `prepare`, `begin_start`, `activate`, `offline`,
`recover`, `begin_stop`, `stop`). They never mutate a cloned Core Route or use
UI status strings as lifecycle authority. An offline completion retains a
structured Route-related Problem and invalidates the current epoch; an explicit
retry always starts with a later epoch.

Platform and child-process callbacks return typed completions through opaque
operation IDs; they never mutate Core state from arbitrary threads.

On Windows, the headless `CapyIOBroker` service selected by ADR 0033 owns the
dedicated virtual-speaker Broker, privileged cross-session render mapping and
child lifecycle. ADR 0034 gives ordinary CapyIO Desktop a bounded local
start/stop/status boundary; Tauri is not privileged and does not own the
service Route's lifetime. Its direct supervisor remains a development fallback.

### Protocol

`capyio.v1` is a versioned semantic control protocol. It does not select a
socket implementation and never carries high-rate frames. Core and wire types
use explicit conversions.

### Adapter SDK and Host

- manifest and control DTOs;
- NDJSON request/response framing;
- sidecar startup/shutdown and bounded stderr capture;
- health/catalog/Route operations;
- ownership mapping from Adapter failure to affected Routes.

### Platform Hosts

Android, Windows, Linux, macOS and iOS translate Runtime operations to platform
permissions, lifecycle and APIs. Mobile platforms may require in-process work;
desktop platforms prefer Sidecars for large or failure-prone components.

### UI

Quick Actions select Route Templates. Workspace exposes the technical graph.
The WebView sees narrow DTO commands only and has no arbitrary shell/filesystem
power.

## 5. Repository module graph

```text
capyio-core
   ^
   +--- capyio-data-plane
   +--- capyio-audio
   +--- capyio-video
   +--- capyio-input
   +--- capyio-protocol
   +--- capyio-runtime
   +--- capyio-adapter-sdk
           ^
           +--- capyio-adapter-host
           +--- mock sidecar binaries

capyio-runtime
   ^
   +--- capyio-testkit
   +--- capyio-node
   +--- capyio-desktop Tauri host

capyio-data-plane
   ^
   +--- SensorServer protocol Adapter (bounded mapping; transport separate)

Audio Share process Adapter
   +--- pinned external as-cmd process (AdapterManaged TCP/UDP PCM)
```

Dependency rules:

- Core depends only on general-purpose value/validation libraries.
- Protocol may depend on Core; Core never depends on generated Protobuf.
- Runtime may depend on Core but not Tauri or platform SDKs.
- Adapter SDK DTOs may depend on Core value types but never on Runtime internals.
- Adapter Host owns process I/O and does not enter Core.
- Profile-specific Adapters may depend on `capyio-data-plane`; the data-plane
  crate never depends on an Adapter or concrete transport.
- `capyio-video` defines exact decoded packed-raw stream candidates, frame
  descriptors, minimal camera metadata and optional metrics. It does not open a
  camera, implement H.264/H.265/RTSP, register a virtual camera or choose a
  platform/data-plane mechanism.
- `capyio-windows-camera` owns the user-mode Media Foundation Projection seam.
  Its first plan is one exact NV12 1280x720 30 fps stream with session lifetime
  and current-user access. Packed frames are copied into bounded platform
  buffers and timestamped from a QPC-correlated 100 ns anchor. The background
  Runtime/Service, not the UI, will eventually own registration and cleanup.
  Its source core has one fixed stream and at most four outstanding sample
  requests. `capyio-windows-camera-mf` is the Windows-only unsafe boundary that
  projects those contracts as an in-process `IMFMediaSourceEx` and
  `IMFMediaStream2`, with separate event queues, a Frame Server-provided sample
  allocator, a Legacy sensor profile and Media Foundation 2D NV12 samples. The
  001B1B gate had no class factory, DLL export or system
  registration backend. CAPY-CAMERA-001B1C adds that boundary in the sibling
  crate as a standard in-process COM server with mandatory
  `IMFGetService`/`IKsControl`/`IMFSampleAllocatorControl`, a fixed
  `IMFActivate` class factory and a closed session/current-user registrar.
  System deployment and activation stay outside normal builds/tests and require
  a hash-recorded lab command plus rollback. CAPY-CAMERA-001B1D adds only a
  validation harness: while the registrar owns one session camera, two bounded
  child processes run sequentially and each independently enumerates and
  activates the public device and consumes two frames. Repeated source `Start`
  calls preserve the active generation, pending requests, allocator and sample
  timeline while publishing the required updated/start events. The host result
  does not establish simultaneous multi-consumer fan-out. Child orchestration
  is not the production video data plane, Runtime owner or future remote-frame
  ingress. CAPY-CAMERA-001B1E adds the first decoded-frame ingress seam without
  changing that process boundary: a worker may submit only the canonical
  720p30 packed NV12 frame contract into a fixed-capacity drop-oldest queue.
  Stream identity, epoch, sequence, timestamp, payload size and live-frame
  flags are validated before ownership transfers. The non-registered MF test
  constructor consumes that queue with non-blocking locks and propagates a
  queue gap as `MFSampleExtension_Discontinuity`. Registered COM activation
  remains fixture-backed until a separate shared-memory/Adapter slice defines
  ownership and security across the Frame Server process boundary.
  CAPY-CAMERA-001B1F defines that first local process boundary without yet
  switching registered activation: a single producer owns the versioned
  `Global\\CapyIO.CameraIngress.v1` mapping, while one or more Frame Server-side
  consumers open read-only views. A fixed 256-byte header and three fixed NV12
  slots consume exactly 4,147,648 bytes. Stream identity, epoch, generation,
  format and every publication are validated; two atomic publication markers
  allow a reader to reject an in-progress/overwritten slot and take only the
  newest stable frame. Consumer cursors are independent and skipped
  publications become discontinuities. The non-registered MF constructor can
  use this provider, but Runtime ownership, registered class-factory selection,
  Android capture, network transport and decoding remain later slices.
  CAPY-CAMERA-001B1G composes the same boundary across two real processes: a
  parent owns and publishes the mapping, while a child opens only the read view,
  starts the non-registered Media Foundation source, allocates a platform video
  sample and observes the published NV12 byte. This establishes composition of
  the IPC and MF sample paths without making the general `capyio-runtime` depend
  on Win32 or extending the audio-specific `CapyIOBroker`.
  CAPY-CAMERA-001B1H performs that dependency extraction.
  `capyio-windows-camera-share` now owns only the fixed Win32 ABI, producer and
  consumer; `capyio-windows-camera-mf` depends on its read side, while
  `capyio-windows-camera-host` owns explicit start/publish/stop lifecycle for
  the write side. The MF consumer can adopt the ACL-protected producer header's
  stream ID and epoch, so registered activation will not need a command-line or
  environment identity channel. The host has no transport, codec, Android API
  or service executable yet.
- `capyio-input` defines normalized pointer/touch/keyboard/gamepad/haptics
  semantics and allocation-free epoch/sequence guards. It does not inject
  input, implement DSU/VIIPER/HID/USB-IP or depend on a platform SDK.
- VCamdroid-compatible encoded video remains an `AdapterManaged` private data
  plane until a complete codec/access-unit contract and explicit decode
  Converter produce `capyio.video.frames/1`.
- The Audio Share Adapter validates and supervises a pinned external executable;
  its TCP/UDP PCM contract does not become a Core, Protocol or StandardPort
  dependency.
- The v0.3.4 CLI may fall back from a requested PCM format to an endpoint's
  default format. Its `AdapterManaged` Route therefore advertises an explicit
  private-negotiated format rather than claiming the requested sample format as
  an observed result.
- Its initial Windows host may observe process-owned established TCP state via
  IP Helper. That platform signal is transport presence only and never imports
  peer addresses, Windows structs or lifecycle decisions into Core.
- The desktop composition layer maps that bounded process signal onto one
  Runtime-owned `AdapterManaged` Route. Consecutive receiver observations are
  required before activation; receiver loss and child exit submit typed
  `Problem`/`Offline` transitions. The polling cadence and retry decision remain
  explicit host policy, and unrelated Routes are never mutated as a side
  effect.
- The versioned Quick Action projection exposes only a stable action ID,
  lifecycle state, evidence label and finite start/retry/stop operations. The
  executable path, endpoint ID and bind address remain trusted host
  configuration. A host-owned worker polls independently of the WebView. A
  bounded number of receiver-wait polls transitions a stuck start to a typed,
  retryable `Offline` Problem and reaps the external process.
- Every Audio Share start re-probes the pinned CLI and current endpoint
  inventory. A configured endpoint that disappeared after RDP, hot-plug or
  audio-service re-enumeration maps to a sanitized, retryable
  `CAPY.AUDIO_SHARE.ENDPOINT_UNAVAILABLE` Problem; raw endpoint IDs remain
  trusted host configuration and do not enter the WebView DTO.
- The desktop may project the freshly enumerated endpoint display names through
  short-lived opaque selection tokens. Only tokens in the host-owned current
  generation map back to endpoint IDs; refresh and successful selection
  invalidate the map. Selection replaces the supervised process configuration
  only while the Route is inactive and is intentionally session-local until a
  trusted persistence design exists.
- Testkit is never a production dependency of Core/Protocol/Runtime/Adapter SDK.
- Drivers communicate through minimal validated contracts, never Rust memory
  layout or network/wire messages.

## 6. Domain graph

```text
Node
  ├─ AdapterInstance*
  └─ Capability*
       └─ Port+

Session: Node ↔ Node trust/control relation
Route: Source Port ──backend/format/QoS/auth──> Sink Port
Problem: related Node/Adapter/Route diagnostic
```

Projections, Panels, Recorders, ROS topics, USB/IP devices and virtual devices
are Adapter-owned Capabilities/Ports. Core does not branch on their mechanism.

## 7. Route lifecycle

```text
Draft -> Prepared -> Starting -> Active -> Stopping -> Stopped
  |          |          |          |          |
  +----------+----------+----------+----------+--> Failed
             +----------+----------+--------------> Offline
```

Specific recovery paths are explicit Runtime commands. A terminal/failure
transition affects only the Route and Adapter ownership set involved.

## 8. Control and data planes

Node-to-node control covers Hello/version negotiation, bidirectional catalogs,
Sessions, Route request/status, authorization, Problems and heartbeat. Sidecar
control uses bounded NDJSON messages on stdin/stdout; stderr is for logs.

Data planes are selected per Route backend:

- `CapyDataPlane`: standard compatible Ports use a CapyIO transport;
- `AdapterManaged`: a vertical integration owns end-to-end data transfer;
- `LocalPipeline`: co-located StandardPorts use same-node conversion, Panel,
  Recorder or Projection;
- `ExternalProtocol`: an explicitly advertising Adapter terminates ROS, USB/IP
  or another ecosystem protocol.

Both endpoint Adapters must advertise the selected backend. `CapyDataPlane` and
`LocalPipeline` require StandardPorts; `AdapterManaged` requires matched
AdapterManaged Ports. `ExternalProtocol` accepts either matched interoperability
mode because a bridge may expose a StandardPort boundary or retain its own
contract. AdapterManaged declarations do not imply arbitrary interoperability
between different Adapter contracts.

## 9. Timing and real-time policy

Profiles may require source/receive timestamps, clock-domain IDs, sequence,
units, coordinates, accuracy and calibration. Audio/video/sensor hardware clocks
are independent. Clock recovery and resampling remain in user mode.

Real-time callbacks use fixed-capacity/preallocated structures. They do not
block, wait on contended locks, allocate without bounds, log normally, parse
JSON/Protobuf, call UI code, or perform network/file I/O.

## 10. Platform projection strategy

```text
P3 system virtual device
  -> P2 system route/injection
    -> P1 standard API/protocol
      -> P0 CapyIO Panel/Recorder
```

This is a user-experience preference, not a promise that every platform exposes
every level. Android/iOS limitations are reported honestly.

## 11. Windows boundary

Any future Windows driver is minimal: endpoint/PCM or fixed IPC surface only.
Networking, DNS, pairing, encryption, JSON/Protobuf, codecs, reconnect and user
configuration remain in user mode. Driver build/install/test defaults to an
isolated VM or dedicated Windows installation. ADR 0029 allows one identified
local-lab exception with recorded recovery posture, exact-package approval and
rollback evidence; it does not relax the driver boundary.

The dedicated remote-speaker projection is a render endpoint named `CapyIO
Speaker`. Windows applications render into it, and an endpoint-associated
render APO copies real PCM into a preallocated bounded shared-memory/SPSC ring
for the user-mode Broker and existing Adapter-managed transport. The APO
real-time callback never blocks, allocates, performs file/network I/O or owns
reconnect policy. SysVAD WASAPI loopback is synthetic and is not real-PCM
evidence; a custom kernel PCM IPC remains a measured fallback only.

The desktop host has two explicit Audio Share launch modes. The legacy mode
supervises pinned `as-cmd` against one host-enumerated playback endpoint. The
dedicated-speaker mode supervises the CapyIO-owned render-ring Broker and has no
endpoint-selection input: `CapyIO Speaker` is the fixed Projection. Trusted
host configuration selects the mode; the WebView cannot supply executable or
network configuration.

For the installed dedicated-speaker flow, `CapyIOBroker` is the privileged
lifecycle owner. CapyIO Desktop sends only bounded `status`, `start` and `stop`
requests over the local named pipe defined by ADR 0034 and projects the typed
service snapshot into its existing Runtime Route. Service launch paths and
network settings remain administrator configuration. Closing the UI does not
stop a service-owned Route; the legacy direct process path is a development
fallback when the service boundary is absent.

## 12. Android boundary

Android platform code owns permissions, microphone indicators, foreground
service, audio focus/routing and hardware APIs. Activity/window lifecycle is not
the eventual Node session lifecycle.

`platform/android/capyio-camera-app` began as the isolated Camera2 lab. Its
Android-free `camera-contract` module owns a deterministic permission/start/
stream/stop state machine. The current `app` module owns one visible `Activity`,
a `TextureView`, one Camera2 device/session and one bounded MediaCodec AVC input
Surface. The repeating request targets only the preview and encoder Surfaces;
the Activity lifecycle still closes the complete capture/encode/export session.

The observation DTO carries only dimensions, source timestamp, sequence,
orientation and facing. Camera2's potentially strided YUV planes are not
claimed to be packed NV12 and are not a `capyio.video.frames/1` data plane. The
lab has no JNI, persistence, foreground service or Node Runtime ownership.
Camera and codec callbacks do not perform socket I/O. Therefore build success
alone establishes the Android boundary, not production pairing, encrypted
transport or background behavior.

CAPY-CAMERA-001C1 adds a sibling `MediaCodecSurfaceEncoder` boundary without
yet connecting it to the Camera2 session. It fixes surface-input AVC at the
selected dimensions/rate/bitrate, requests no B-frames and BT.709 limited SDR,
owns the codec callback thread/input Surface, and copies each output into an
owned access unit capped at 4 MiB. A queue of at most eight access units uses a
non-waiting lock attempt: it drops the incoming unit on contention and drops
the oldest on capacity, with both losses observable. Codec parameter sets are
bounded to 64 KiB each. The encoded output remains a private
`AdapterManaged` payload; it is not yet an RTSP/RTP or StandardPort contract.

CAPY-CAMERA-001C2 composes the two Android boundaries. Camera inventory selects
one positive even size advertised for both `SurfaceTexture` and `MediaCodec`,
preferring the largest common size at or below 1280×720. The repeating Camera2
request targets only the visible preview and encoder input Surface. Capture
results provide the sensor timestamp/sequence observation; the camera thread
drains at most eight encoded queue entries per result and forwards only
metadata to the Activity. Camera session/device shutdown precedes codec/input-
Surface release. This removes raw YUV conversion from the intended encoded
path, but actual HAL stream-combination and encoder negotiation remain device
facts rather than build-time guarantees.

CAPY-CAMERA-001C3 fixes the first cross-language encoded-camera boundary without
selecting a network. `camera-contract` encodes a private 56-byte-header AVC
config/access-unit record; `adapters/vcamdroid` decodes the same golden bytes
and guards config-first, exact stream/epoch, advancing sequence/timestamp,
key-frame discontinuity and terminal EOS semantics. The format remains wholly
`AdapterManaged`; Core, `capyio-video`, Protobuf, JSON-RPC and Windows camera
mapping code do not parse it. A later authenticated transport owns delivery,
while a decoder Adapter converts accepted access units into the existing packed
NV12 Windows camera-host boundary.

CAPY-CAMERA-001C4 adds a deliberately narrow transport lab around that record.
`AvcWireSessionEncoder` detects Annex-B, four-byte-length-prefixed and AVC
decoder-configuration layouts off the codec callback, emits config before the
first accepted key frame and converts observed sequence loss into key-frame
discontinuity recovery. `LoopbackAvcSender` accepts access units through an
eight-entry non-waiting queue, while its worker alone allocates wire records and
writes a socket. The Android destination is fixed to device loopback port 38173;
an explicitly configured ADB reverse tunnel carries it to a Rust executable
listening only on Windows loopback. The Rust stream reader caps the payload
length before allocation, rejects short records and applies the C3 guard before
reporting counters. This is an authenticated-debug-channel lab boundary, not a
LAN listener, reconnect protocol, production Route transport or decoder. No
network or codec logic enters Core, JSON-RPC, shared-memory/MF code or a driver.

CAPY-CAMERA-001C23 adds an opt-in ADB-free trusted-LAN lab without changing the
CAVC record or any Core boundary. Android keeps blank input mapped to the C4
loopback destination. A non-empty destination is accepted only as a canonical
IPv4 literal in RFC1918, link-local or 100.64.0.0/10 space and remains fixed to
port 38173. Windows trusted-LAN mode is representable only when an exact local
bind IPv4 and a different exact allowed peer IPv4 are both supplied; wildcard
binds, DNS, discovery, public addresses and caller-selected ports are absent.
The closed `trusted-lan-live-hold` command passes only those two addresses to
the fixed sibling receiver and otherwise retains C14 registration, liveness and
cleanup behavior. This is a plaintext trusted-lab transport, not a production
Route: mutual authentication, authenticated encryption, Route/Session binding,
replay protection and downgrade binding remain required before untrusted use.

CAPY-CAMERA-001C24 hardens only the foreground lab lifecycle. Android retains a
fixed-capacity media path but extends its fixed connection budget to 120
attempts and keeps the display awake while the visible capture state is active;
pause still closes the whole session. Windows waits at most 120 seconds for the
first validated mapping and retains the same publisher for a fixed 60-second
peer-reconnect grace. The orchestrator always evaluates child and mapping
cleanup, and a cleanup failure takes precedence over an earlier validation
error. No timeout is caller-controlled and no reconnect logic enters Core, the
COM callback or a driver. Camera choices remain directly openable Camera2 IDs
plus explicitly labelled vendor Zoom targets; a Zoom target is not physical-
sensor identity.

CAPY-CAMERA-001C25 keeps configuration-only rotation inside the same visible
Android Activity. The manifest delegates orientation, screen-size and smallest-
screen-size changes to `MainActivity`; its handler refreshes UI state without
changing the Camera2/MediaCodec/export session. A true Activity pause or preview
surface loss still crosses the existing foreground boundary and closes the
complete session. The Activity does not lock orientation, persist the trusted-
LAN address, introduce a service or move lifecycle state into Core.

CAPY-CAMERA-001C5 adds the first decoder Adapter on Windows without widening the
transport. After the C3 guard accepts an Annex-B config, the Adapter configures
the inbox H.264 Media Foundation Transform with bounded frame metadata and SPS/
PPS, prefixes those parameter sets to the first key frame after each stream
start/discontinuity, and requests NV12 output. It handles backpressure, bounded
output type changes, flush and drain while mapping output timestamps back to
the accepted source sequence. Strided output is copied into a bounded packed
NV12 allocation. The loopback executable immediately hashes and drops each
decoded frame. Core, JSON-RPC, the Android callback, shared-memory mapping and
the virtual-camera COM source do not run the codec; connecting these decoded
frames to the existing camera-host producer is a subsequent slice.

CAPY-CAMERA-001C6 composes that decoder with `CameraProducerHost`. Decoded
identity, epoch, source sequence, timestamp, discontinuity and payload move into
the existing canonical `GeneratedVideoFrame` and then the versioned triple-slot
mapping. A compile-time lab feature exposes one exact current-session Local
name for unprivileged cross-process evidence; it accepts no arbitrary name and
registered activation never opens it. Production activation opens only the
fixed Global mapping, uses its validated header when present, falls back to the
fixture only for file-not-found and fails every other open/header error. The
current user token cannot create that Global mapping, so its owner must be a
separately controlled privileged camera host/service rather than the portable
Runtime or ordinary Adapter process.

CAPY-CAMERA-001C7 closes the registered live-source scheduling gap. The shared
provider accepts at most four `RequestSample` calls into a Media Foundation
serial work queue. A request that arrives between 30 fps publications retains
its FIFO token and is retried by a 5 ms scheduled work item; the COM callback
does not sleep, grow an unbounded queue or treat normal producer cadence as a
fatal stream error. Fixture and process-local ingress providers retain their
existing synchronous behavior. The controlled Global roundtrip proved Frame
Server can activate this source and return advancing V2419A-backed NV12 samples.

CAPY-CAMERA-001C26 closes only the registered late-producer selection gap. If
activation finds the fixed Global mapping absent, the registered source uses an
asynchronous late-bound provider instead of permanently selecting its fixture.
That provider emits the existing deterministic 720p30 fixture as an offline
placeholder and, after a fixed 15-placeholder-frame countdown, attempts only
the production mapping from the existing MF serial work queue. The first
validated shared frame is rebased onto the active virtual-camera stream/epoch,
sequence and timeline and is marked discontinuous; later empty reads retain the
bounded shared-sample retry and never insert placeholder frames into live mode.
An already-present mapping still selects the direct shared provider, and every
open/header error except exact file-not-found fails activation or the pending
request. This does not create persistent registration, service ownership,
producer-loss detection or any new control/network path.

CAPY-CAMERA-001C27 supersedes that one-way provider selection for registered
activation. Both the initially present and initially absent mapping cases now
enter one asynchronous provider lifecycle. While live, 400 consecutive empty
polls on the existing 5 ms serial sample pump form a bounded nominal two-second
stall window. Expiry releases this consumer's mapping handle and resumes the
deterministic fixture at the virtual timeline's next sequence and timestamp;
the transition is discontinuous. The existing fixed 15-placeholder-frame probe
interval then permits a newer publication from the paused producer or a new
mapping generation to reattach without recreating the Media Foundation source.
Producer generation/source identity plus pre-rebase source sequence/timestamp
prevent a reopened mapping's last publication from being replayed. Direct
caller-owned in-process shared providers remain unchanged. The mechanism is a
sample-demand-driven liveness policy, not a heartbeat, service owner or new IPC
ABI; another consumer may still keep an abandoned mapping alive.

## 13. Failure isolation

- bounded parser sizes and queues at every process/network boundary;
- first terminal operation completion wins cancellation races;
- child exit produces a structured Problem and fails only owned Routes;
- peer disconnect creates Offline state and a fresh epoch on recovery;
- UI failure cannot take down Node Runtime state;
- driver absence never causes Runtime to parse untrusted data in kernel space.

## 14. Evolution and compatibility

- Protobuf fields append within major v1 and removed numbers remain reserved;
- Profile breaking semantic changes require a new Profile major;
- manifest/control protocol versions negotiate independently;
- public Core diagnostic JSON is not automatically a public wire schema;
- third-party data planes can be progressively replaced by StandardPort
  integrations only when interoperability is real and tested.

## 15. Foundation status

Gates 0–3 implement and test the domain, Mock UI and mock Sidecar only. Real
hardware Adapters, production networking/security, Android services, Windows
virtual devices, third-party source and performance claims are deferred to the
roadmap.
