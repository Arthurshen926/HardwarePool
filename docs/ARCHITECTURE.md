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
- accepts a complete peer catalog only through a Ready Session bound to that
  exact remote Node identity; the replacement and all dependent Route
  reconciliation commit atomically, while transport authentication and
  envelope decoding remain outside Runtime;
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

ADR 0040 deliberately splits microphone ownership by privilege. The same
`CapyIOBroker` service owns the global capture mapping required by AudioDG,
while an ordinary-user `capyio-microphone-host` owns the user-local MicYou
configuration and child process. Desktop controls that host through a separate
bounded owner-scoped pipe and does not stop it on window shutdown. Direct
desktop supervision remains a development fallback.

### Protocol

`capyio.v1` is a versioned semantic control protocol. It does not select a
socket implementation and never carries high-rate frames. Core and wire types
use explicit conversions.

### Control Host Composition

`capyio-control-host` is the transport-agnostic seam between an already
admitted peer connection, `capyio-protocol` decoding and `NodeRuntime` state.
It binds one exact Session/peer tuple, applies only complete CatalogSnapshot
messages and retains at most 256 recent message IDs for accidental duplicate
suppression. Platform hosts still own authentication, cryptographic replay
protection, deadlines, framing I/O and reconnect.
Catalog bytes crossing this seam are independently bounded by both the 1 MiB
Envelope ceiling and fixed descriptor, metadata and per-string limits before
Core conversion. These parser bounds constrain host work but do not establish
the admitted binding or replace transport authentication.

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
The Windows `capyio-process-presence` helper confines bounded owner-PID TCP
table FFI behind a safe count-only API. It never enters Core or Runtime and does
not expose peer addresses. The Windows `capyio-micyou-host-config` boundary
loads only a complete development override or a schema-versioned file below a
fixed user-local CapyIO path. It is not a WebView filesystem API.
`capyio-microphone-host` consumes that boundary as the ordinary user and never
accepts a launch path or endpoint identity from its local control client.

### UI

Quick Actions select Route Templates. Workspace exposes the technical graph.
The WebView sees narrow DTO commands only and has no arbitrary shell/filesystem
power.

The Windows Tauri process owns one `DesktopNodeState` containing the
`NodeRuntime`, bootstrap Session identity and demo Route allow-list. `DemoLab`
is consumed only to construct deterministic startup data; background audio,
microphone, IMU and gamepad workers share the process-owned Runtime directly.
This removes the test-fixture wrapper from the long-lived host ownership
boundary so a later authenticated control connection can mutate the same
Runtime. It does not itself create a connection or grant peer trust.

ADR 1400 makes the Vue 3/TypeScript presentation shared across the Windows and
future Android Tauri shells while keeping each platform's Runtime service,
permissions, hardware APIs and real-time workers native. `UiSnapshot` schema 4
projects local and peer Capability catalogs with explicit Port directions. The
UI separates the tasks “share local capabilities” and “use peer capabilities”
without inventing global sender/receiver Node roles. A local Source checkbox is
publication intent only; Session trust, Route authorization, permission and
worker lifecycle remain distinct host-owned operations. The current Tauri
catalog is fixture-backed and must remain labeled simulated until an Adapter
has actually enumerated the host.

Before a local catalog is encoded for an admitted Session, the shared control
host applies that publication intent as a complete-Capability projection.
Capabilities containing any Source Port are retained only when selected;
Sink/Control-only Capabilities remain visible because the Source checkbox does
not control them. Unknown, non-Source or Offline selections fail closed, and
Adapter ownership is pruned and revalidated after filtering. A mixed-direction
Capability is indivisible and is omitted as a whole when unselected.
The final outbound encoder is a method on the same admitted peer-control object
that processes inbound catalogs, so platform code cannot independently choose a
different Session ID for publication.
Windows desktop selection now executes this shared projection against a cloned
candidate set before committing process-local intent. A failed projection leaves
the previous selection unchanged, while deselection remains possible after a
previously selected Source becomes Offline.

The built-in Controller view is shared presentation code for desktop and a
future Android host. It emits closed semantic control changes; a trusted host
owns stream identity, epoch, sequence and complete-state composition. Desktop
may expose an explicitly simulated Controller source and IPv4-loopback DSU
diagnostic, but that fixture is not a peer data plane or Runtime Route claim.
Its Windows status surface may also request one parameter-free read-only host
preflight. Fixed host configuration probes the pinned VIIPER identity and
usbip-win2 Xbox export list; the WebView cannot supply an executable, endpoint,
bus or attachment port and cannot reach any mutation through this command. The
probe is single-flight and runs outside the shared gamepad-state lock; only its
finished immutable status projection is published to the UI.

## 5. Repository module graph

```text
capyio-core
   ^
   +--- capyio-data-plane
   +--- capyio-audio (shared direction-neutral audio contracts/algorithms)
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

capyio-control-host
   +--- capyio-protocol
   +--- capyio-runtime
   +--- no socket, crypto or platform dependency

capyio-data-plane
   ^
   +--- SensorServer protocol Adapter (bounded mapping; transport separate)

Audio Share process Adapter
   +--- common selected Speaker stream specification and metrics
   +--- pinned private AdapterManaged TCP/UDP PCM binding

MicYou process Adapter
   +--- common selected voice-interactive stream specification and metrics
   +--- pinned private AdapterManaged TCP/UDP microphone binding

CapyIO audio media seam
   +--- Session/Route/Stream/epoch/exact-spec binding
   +--- direction-neutral bounded PCM/encoded packets
   +--- bounded worker-thread reference queue
   +--- replaceable compatibility/native transport Adapters

capyio-process-presence
   +--- bounded Windows process-owned TCP observation for platform hosts

capyio-micyou-host-config
   +--- fixed-path trusted MicYou launch configuration and provisioning CLI

capyio-windows-service
   +--- privileged Speaker/global-ring service host
   +--- ordinary-user headless microphone-host binary
   +--- shared bounded local named-pipe transport
```

Dependency rules:

- Core depends only on general-purpose value/validation libraries.
- Protocol may depend on Core; Core never depends on generated Protobuf.
- Runtime may depend on Core but not Tauri or platform SDKs.
- Adapter SDK DTOs may depend on Core value types but never on Runtime internals.
- Adapter Host owns process I/O and does not enter Core.
- Profile-specific Adapters may depend on `capyio-data-plane`; the data-plane
  crate never depends on an Adapter or concrete transport.
- `capyio-audio` defines complete selected audio candidates, bounded QoS
  policies, decoded frames, a direction-neutral Session/Route/Stream/epoch
  media binding, bounded PCM/encoded packets, worker-thread queues,
  reordering/clock estimates and common metrics for both microphone and speaker
  Routes. Its packet is a semantic in-process value rather than a network byte
  layout. The crate does not define a global audio Source/Sink role, open
  sockets, access platform audio APIs, run codecs or select a production
  transport.
- ADR 0042 requires every audio transport Adapter to expose one validated
  backend contract covering interoperability, media access, supported encodings,
  per-field metadata fidelity and observable security. A StandardPort backend
  cannot validate unless it carries the full common packet exactly.
- Initial audio negotiation selects the first Source-preferred complete
  candidate also advertised by the Sink. It never silently resamples, changes
  encoding, enables processing or rewrites QoS; a later Converter must make any
  such operation explicit.
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
- `capyio-input` defines normalized pointer/generic-touch/keyboard/gamepad/
  haptics semantics plus physical five-contact touchpad frames and
  allocation-free fail-safe epoch/sequence guards. It does not inject input,
  implement DSU/VIIPER/HID/USB-IP or depend on a platform SDK.
- `capyio-windows-input` is the host composition boundary for DSU and VIIPER
  Projections. The DSU path registers a typed StandardPort IMU sink, owns one
  bounded IPv4-loopback Worker per Runtime `ExternalProtocol` Route and
  activates only after bind plus exact-epoch anchor validation. The VIIPER path
  registers a typed gamepad sink and activates only after provisioning plus
  initial neutral succeed. Both complete cleanup before Offline/Stopped,
  retries use a new Route epoch, and unrelated Routes are never mutated as a
  side effect. The SensorServer-to-DSU executable is an explicit physical-lab
  surface, not a production UI/data-plane transport.
- Its optional usbip-win2 owner is also user mode: it launches one absolute
  pinned CLI directly, never through a shell; accepts only IPv4 loopback and
  the VIIPER-derived Xbox 360 export; bounds process time/output; attaches with
  no persistent retry; and retains one exact hub port. A USB/IP-enabled Route
  does not become Active until attachment succeeds. Stop writes neutral before
  exact-port detach and owned-bus removal. Driver install/remove, reboot and
  boot/security policy remain outside the Runtime Route.
- A separate read-only desktop preflight reuses those bounds but returns only
  exact Xbox 360 exports and sanitized readiness. Zero matches is not an error,
  one match is visible as `export_ready`, and multiple matches fail closed. It
  neither selects an export for attachment nor owns a Route lifecycle.
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
- The CapyIO-authored Audio Share-compatible sender additionally binds a common
  media Route epoch and validates each PCM packet before stripping the metadata
  its pinned private wire cannot carry. Its backend contract declares exact
  payload only, partial format mapping and absent common identity/timing.
- The MicYou backend contract is explicitly opaque: CapyIO binds its process to
  one conservative voice Route epoch for lifecycle but neither observes nor
  claims a common media packet. Private PCM/Opus capability is not exact common
  stream negotiation evidence.
- Initial Windows audio hosts may observe process-owned established TCP state
  through the ADR 0038 platform helper. That signal is transport presence only
  and never imports peer addresses, Windows structs or lifecycle decisions into
  Core, Runtime or either Adapter.
- The desktop composition layer maps that bounded process signal onto one
  Runtime-owned `AdapterManaged` Route. Consecutive receiver observations are
  required before activation; receiver loss and child exit submit typed
  `Problem`/`Offline` transitions. The polling cadence and retry decision remain
  explicit host policy, and unrelated Routes are never mutated as a side
  effect.
- The versioned Quick Action projection exposes only a stable action ID,
  lifecycle state, evidence label and finite start/retry/stop operations. The
  executable path and endpoint ID remain trusted host configuration. A complete
  environment override is accepted for development; otherwise the host reads a
  deny-unknown-fields schema-v1 file at its fixed user-local path. MicYou may
  show its validated bind IP/port only as phone connection guidance. A
  host-owned worker polls independently of the WebView. A
  bounded number of receiver-wait polls transitions a stuck start to a typed,
  retryable `Offline` Problem and reaps the external process.
- Every Audio Share start re-probes the pinned CLI and current endpoint
  inventory. A configured endpoint that disappeared after RDP, hot-plug or
  audio-service re-enumeration maps to a sanitized, retryable
  `CAPY.AUDIO_SHARE.ENDPOINT_UNAVAILABLE` Problem; raw endpoint IDs remain
  trusted host configuration and do not enter the WebView DTO.
- The MicYou desktop composition registers an Android microphone Source and a
  Windows `CapyIO Microphone` Sink as an independent `AdapterManaged` Route.
  Listener readiness leaves it `Starting`; consecutive process-owned phone TCP
  observations activate it. Active peer loss stops and reaps the receiver
  before reporting `Offline`, while bounded initial connection wait and
  explicit retry use typed Problems and fresh epochs without mutating Speaker
  or IMU Routes. The capture ring may drain only its bounded already committed
  frames before exact silence. TCP presence is not recorded as PCM evidence.
- When the ADR 0040 per-user host is present, desktop maps its typed snapshot
  back through the same `MicYouProcessBoundary` and Runtime Route rather than
  creating a second lifecycle model. The host independently bounds phone wait
  and disconnect cleanup; desktop adopts an already-running host and does not
  stop it during UI shutdown. The local request remains finite
  `status`/`start`/`stop` and carries no executable or endpoint authority.
- The MicYou host configuration CLI probes the separately supplied executable,
  selects an explicit stable endpoint ID and derives its expected name before
  writing a new file. It never persists a device index and never silently
  overwrites existing configuration. Every actual start still re-probes the
  external process and freshly resolves ID to the current index.
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

ADR 0035 applies one media contract to both audio directions while ADR 0004
keeps microphone and speaker as independent Routes. ADR 0041 binds every media
channel to one Session, directed Route, Stream, positive epoch and exact
selected specification before a concrete transport is entered. Opposite Routes
may share a Session but never a media queue, epoch or failure state. A future
duplex association may expose a render reference to a capture-side AEC
implementation, but it does not merge permissions, stop state or failure state.

## 10. Platform projection strategy

```text
P3 system virtual device
  -> P2 system route/injection
    -> P1 standard API/protocol
      -> P0 CapyIO Panel/Recorder
```

This is a user-experience preference, not a promise that every platform exposes
every level. Android/iOS limitations are reported honestly.

For Windows Precision Touchpad projection, ADR 1100 selects a runtime-loaded
user-mode synthetic touchpad as the first feasibility path. The platform
Adapter, not Core or `capyio-input`, owns `user32.dll` discovery, native
structure translation and device-handle cleanup. API availability alone is not
a gesture-compatibility claim. VHF remains a separately reviewed fallback and
still requires a minimal driver boundary, isolated deployment plan and physical
acceptance evidence.

Physical acceptance subsequently showed complete tap, one-finger and
two-finger behavior but no configured Shell action from accepted three- or
four-contact input. ADR 1107 therefore activates a compile-only VHF fallback.
That driver exposes only the mandatory HID collections, fixed bounded reports
and a small versioned local control ABI. The user-mode Broker retains all
network/session validation and gesture-independent touchpad projection. Driver
deployment remains outside this architecture slice.

ADR 0043 keeps Android `MotionEvent` and Windows `POINTER_TYPE_INFO` in their
platform crates. Android emits post-event complete snapshots after an explicit
epoch cancellation. Windows differences those snapshots into at most two
fixed-capacity batches, immediately cancels retained native contacts after a
gap/epoch change, and encodes PT_TOUCHPAD himetric fields without copying a
foreign clock into Win32 timestamp fields. Encoding is testable without calling
the operating-system injection function.

The Android runtime-facing capture session wraps the DTO mapper with a bounded
five-pointer lifecycle. It rejects identity drift across callbacks and emits
explicit cancellation on start, lifecycle stop and close. State changes commit
only after a frame maps successfully. The session creates no Android component,
thread, queue or transport. The later 002U/002V composition supplies JNI copying
and a lab Activity outside this pure capture crate.

Android multi-finger motion conditioning is spatial, bounded and
gesture-independent. One-/two-contact snapshots remain identity mapped. When a
gesture first reaches three contacts, fixed storage records raw/output anchors
without changing that frame; later deltas use 700-per-mille gain until the last
contact releases. Pointer reordering and additions preserve per-ID anchors and
all arithmetic clamps to the declared surface. The lab Activity may suppress
MOVE briefly while added contacts settle, but never suppresses pointer lifecycle
frames. Windows remains the only gesture recognizer.

The Adapter-owned packet Source is the next boundary after capture. It requires
the initial cancellation barrier, rejects sequence gaps transactionally and
will not close while the latest accepted snapshot retains contacts. Packet
framing therefore remains outside the Android platform crate, while network
authentication and delivery remain outside both capture and Source.

The delivery session composes that Source with a host-supplied admitted-channel
trait. It rechecks the complete Route binding before each send and distinguishes
definite rejection before write from an unknown delivery result. Only the first
case permits the same frame to be retried; ambiguity, admission loss or binding
drift closes the channel and faults the session. This is transport policy and
ownership scaffolding, not a pairing, encryption or socket implementation.

The sender-side Runtime delivery worker then owns when those checks are made.
Its read-only provider and coherent clock are composition-owned; construction,
each frame and normal close fetch current state internally. The worker derives
an exact Active Source binding from the Runtime Route and compares it with the
initially admitted Route/Session/endpoints/epoch/lease tuple before asking the
channel to reassert the same tuple. Provider/clock failure, rollback, expiry or
Route lifecycle/identity drift closes the channel and faults the sender. Like
the receiver worker, this is a deterministic caller-driven object rather than a
thread, timer, socket or transport implementation.

The private host channel provides a concrete bounded composition between the
sender and receiver workers without selecting a network stack. One preallocated
queue retains at most 64 fixed-size packets and is shared only by caller-driven
single-threaded handles. Admission denial, binding replacement or receiver
close clears queued state; capacity exhaustion and known disconnect reject
before write. Orderly sender close allows already accepted packets to drain.
This is an AdapterManaged in-process handoff used to validate the complete
Runtime-to-Windows projection path, not CapyDataPlane interoperability or peer
authentication.

Controlled 002S evidence substitutes the real Windows synthetic-touchpad Sink
for the hardware-free projector at the end of that same composition. A fixed
one-finger lifecycle crosses the Runtime sender, four-packet host queue and
Runtime receiver before native submission and immediate teardown. The exact
test remains ignored by default and does not broaden the channel into a remote
transport or authorize arbitrary contact input.

ADR 0045 adds a bounded record codec between that sender/channel contract and a
future authenticated reliable stream. Hello repeats the complete Runtime
binding, Data couples the outer epoch/sequence to one private packet, and exact
Ack/Close records make completion explicit. This remains a pure Adapter codec:
the trusted Node composition must supply mutual authentication, encryption,
deadlines and I/O scheduling. ACK ambiguity faults the Route instead of risking
a repeated click or gesture.

ADR 1108 composes those records with the packet receiver and an injected
platform Sink factory. Side-effect-free construction validates the semantic
contract; only an exact Hello can open the Sink. Data receives an Ack only after
the existing packet/sequence/rate checks and Sink submission. Any malformed
transport record closes an active Sink and makes the connection terminal. The
composition remains deterministic and I/O-free; it does not replace Runtime
Route admission, peer authentication or a future service pipe policy.

CAPY-PTP-002U/002V add the first Android composition without changing those
ownership rules. A Kotlin Activity copies complete `MotionEvent` primitives into
a narrow JNI record session; Rust retains lifecycle, packet and transport-codec
state. A 64-record Android queue feeds a dedicated socket thread. For the
physical lab only, ADR 1106 supplies an ADB-paired reverse tunnel to a Windows
loopback-only listener. The listener validates Hello before opening the Windows
Sink and emits Ack after native submission through the ADR 0049 receiver. This
is an isolated lab composition, not the production Runtime transport or Android
service architecture.

The lab also makes a phone-side ownership boundary observable: if Android
cancels a stream after at least three contacts, the Activity records a probable
OEM gesture interception and may route the user to the vendor settings UI. It
does not synthesize the cancelled contacts or move gesture interpretation out
of Windows.

The controlled local injection harness is not a Runtime or UI API. It accepts
only four compiled, one-shot fixtures, defaults to native-structure dry-run and
requires a separate desktop-impact acknowledgement with explicit injection
mode. One RAII owner retains the System32 module and synthetic device handle,
submits only validated batches, attempts bounded cancellation on failure and
destroys the device before unloading user32. Production Route integration must
not reuse this lab CLI as an authorization boundary.

`SyntheticTouchpadSession` is the reusable Windows Sink lifecycle beneath that
CLI. It owns exactly one projector and synthetic device, accepts only validated
complete touchpad frames, rejects frames after failure/close, and attempts
bounded cancellation on explicit close, failed submission and abandoned drop.
Epoch changes are explicit. Network, pairing, Route authorization, reconnect
and UI policy remain outside the session and must be supplied by the Node
Runtime/Adapter boundary.

ADR 1103 adds a private AdapterManaged packet between the Android mapper and
Windows projector without turning it into a Core, Protobuf or public
CapyDataPlane layout. One codec is bound to the Route's Stream, epoch and
touchpad descriptor; its fixed little-endian representation is at most 152
bytes. The codec performs no I/O or trust establishment. A future transport
must authenticate/authorize the peer and bind Route/Stream identity before
receiver construction.

`PrivateTouchpadReceiver` is the Adapter-owned user-mode lifecycle between that
future transport and a Sink. It applies trusted-local receive-time monotonicity,
a configured fixed packet-rate window, stream sequence/epoch checks and an
active-contact idle deadline before calling the Sink. Forward gaps remain
observable and deliberately enter the projector so it cancels retained
contacts; duplicate/late or malformed packets never enter the Sink. Every
fault, timeout, disconnect or abandoned receiver attempts bounded Sink close.
The receiver opens no transport, schedules no poll and does not authorize its
own construction; those remain Node Runtime/Adapter responsibilities.

`PrivateTouchpadRouteSession` then binds that receiver to the current Core
Route without adding touchpad concepts to Core or Runtime. The Adapter checks
the exact Route/Session/endpoints, local Sink, AdapterManaged backend, touchpad
Profile, authorization expiry, Active state and epoch on each enqueue/pump.
Its preallocated logical queue holds 1..=64 fixed-size records and one pump is
bounded to that capacity. Queue overflow, a record delayed to the active-idle
deadline, local-clock regression or any Route mismatch closes the receiver.
Starting-to-Active and later-epoch transitions remain explicit Runtime-driven
commands. The object creates no worker and opens no socket.

`PrivateTouchpadRuntimeWorker` supplies the next Adapter-owned composition
boundary without adding a Runtime dependency to production Adapter code. A
read-only provider returns one owned Core Route snapshot and a monotonic clock
returns one coherent millisecond/nanosecond sample per action. The caller drives
activate, enqueue, tick, epoch and stop commands; the object creates no thread,
timer, socket or OS device. Provider/clock failure, rollback and Route drift are
fail-closed. `NodeRuntime` integration is verified through a dev-only provider
adapter, preserving the Core to Runtime/Adapter dependency direction.

Platform Sink creation is mediated by `PrivateTouchpadSinkFactory`. The worker
performs a side-effect-free validation pass over the current authorized Route,
Stream contract and all bounds before invoking the factory. On Windows the
zero-sized production factory opens `SyntheticTouchpadSession`; invalid Route or
contract input therefore cannot reach user32 device creation. The factory is a
type-level composition path only until an authorized caller explicitly invokes
it.

Controlled host evidence now proves this composition path can create and close
the Windows synthetic device after a real `NodeRuntime` Route preflight. The
acceptance remains an exact-name ignored test and submits no touchpad frames;
normal tests and CI retain a zero-device default.

With separate approval, a second exact ignored test activates the same real
Route and passes CancelAll/down/move/release through codec, bounded worker and
Windows Sink. This proves the full in-process submission/lifecycle composition;
it does not replace live transport, Android capture or physical gesture
observation.

The same ignored composition harness now also submits a closed two-contact
vertical pan with stable identities. No gesture recognizer is added to CapyIO;
Windows remains responsible for interpreting the Precision Touchpad contact
geometry as scrolling.

The three-contact acceptance likewise preserves raw contact geometry rather
than encoding a CapyIO gesture command. A fixed horizontal swipe with stable
IDs crosses the Route, codec, bounded worker and Windows Sink; Windows remains
the sole gesture recognizer and policy owner for desktop/application switching.

The four-contact acceptance uses the same architecture and cleanup path. It
completes the bounded native-submission matrix for one through four contacts;
it does not add gesture semantics to the protocol or prove the user's current
Windows four-finger policy produced a visible action.

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

The microphone projection now has two mutually exclusive producer modes. In
compatibility mode, privileged `CapyIOBroker` owns only the fixed global
capture ring while an ordinary-user `capyio-microphone-host` owns the MicYou
process and ADR 0039 configuration. In native mode, the Broker owns that ring
and supervises `capyio-native-virtual-microphone` beside the native speaker
child. The service configuration, not the WebView, fixes both explicit peer
endpoints. The ring remains single-producer/single-consumer: compatibility and
native capture producers must never run concurrently. Independent native
microphone Route control and per-user compatibility-host installation remain
future work.

ADR 0048 distinguishes native-audio pair readiness from process liveness after
transactional startup. If exactly one native child exits, the Broker returns to
`starting` but does not stop the child that still owns the opposite Route. Only
loss of both children is treated as a terminal pair exit. Automatic
per-direction restart and direction-specific public diagnostics remain 001G
work rather than implicit behavior.

## 12. Android boundary

Android platform code owns permissions, microphone indicators, foreground
service, audio focus/routing and hardware APIs. Activity/window lifecycle is not
the eventual Node session lifecycle. `CAPY-AUDIO-NATIVE-001` targets one CapyIO Android Node
service with independent microphone Source and speaker Sink Adapters behind the
ADR 0041 media seam.

ADR 0043 and `CAPY-AUDIO-NATIVE-001C` now provide the first buildable platform
shell. A non-exported, `START_NOT_STICKY` foreground service owns real
`AudioRecord` and `AudioTrack` handles; the Activity observes a narrow
schema-v1 snapshot and requests independent operations. Each capability has a
separate generation so a late platform completion cannot reverse Stop or fail
the opposite direction. Actual Android parameters are retained rather than
assuming the requested 48 kHz PCM mode was granted.

ANDROID-A01B1 adds a dependency-free, schema-v1 Android-local Capability
catalog contract. A bounded registry is owned by the Node service and exposed
through its existing Binder observer boundary. The audio module projects its
live microphone Source and speaker Sink state; camera, controller and touchpad
modules are declared as unattached and therefore remain explicitly Offline.
This is a migration inventory rather than hardware discovery. The DTO contains
only bounded metadata and never carries platform handles or media payloads.
ANDROID-A01B2 separates read-only `HardwarePresence` from Adapter attachment and
Capability availability. At service creation the Android host queries audio
outputs, the microphone/camera/touchscreen feature inventory, accelerometer plus
gyroscope presence and vibrator availability without opening any device. A
detected camera or sensor therefore remains Offline while its Adapter is not
attached; absence receives a distinct bounded Problem code.
Probe exceptions fail closed as `Unknown` with a separate Problem code and
cannot make the Capability selectable.
ANDROID-A01B3 begins lifecycle attachment with the IMU Source. A reusable,
generation-bound non-audio lifecycle controller is owned by the Node service;
its Android Adapter registers the accelerometer and gyroscope and retains only
one fixed-size latest sample. Sensor callbacks perform no network, file or UI
work. The Binder exposes start/stop commands and observer state, while the
catalog reports the attached IMU as Available, Busy or Offline independently of
hardware presence. This slice does not yet route samples onto a data plane or
claim background/lock-screen operation.
The next A01B3 slice attaches the touch-gamepad control-state boundary without
copying the lab UDP sender into the product host. The service owns one complete,
bounded control snapshot and a generation-bound lifecycle. A UI client submits
whole frames through the local Binder with the observed generation; Stop
neutralizes every control before completion, and stale queued UI frames cannot
enter a retried session. The shared touch surface and product data-plane Route
remain separate follow-on work. Android vibration remains unattached because
calling `Vibrator` requires a Manifest permission change and therefore separate
human approval.

ANDROID-A01C1 establishes the presentation packaging seam without changing the
Android Activity or Node lifecycle. The Android Gradle build invokes the pinned
workspace Vue/Vite build from `apps/desktop`, stages that exact output under
`assets/capyio-ui`, and rejects an absent entrypoint, absent JavaScript/CSS, or
root-absolute asset references. Vite therefore emits relative asset paths that
also remain valid in the Windows Tauri host. This proves one presentation source
is packageable on both platforms; it does not yet render that presentation on
Android or expose Binder commands to JavaScript.

ANDROID-A01C2 adds that local host without replacing the proven native audio
controls. The Activity can reveal the packaged Vue workspace in a WebView and
return to the native controls with Back. The shared API reports `android_node`,
projects the service-owned schema-v1 catalog into shared `UiSnapshot` schema 4
and does not fabricate a peer catalog or Routes. Later A01D/E slices add only
closed local lifecycle, state and publication-intent methods. Service state
notifications refresh the snapshot while platform services continue to own
every Adapter lifecycle.

ANDROID-A01D1 adds the first mutable shared-UI operations without conflating an
Adapter lifecycle with a Route. Two schema-v2 Quick Actions have kind
`capability_control` and target only the attached IMU and gamepad Adapters. The
bridge admits exact action IDs plus state-dependent `start`, `retry` or `stop`;
all other IDs, operations and invalid transitions fail closed. Commands enter
the existing Binder methods and are executed by the Service main handler. They
do not select sharing, authorize or create a Route, transmit samples, request a
permission or submit high-rate gamepad state. Audio permission UX and Route
composition remain later A01D slices.

ANDROID-A01D2 connects the shared Controller surface to the existing
Service-owned gamepad facade. The JavaScript bridge accepts only five primitive,
bounded command shapes; a dependency-free mapper combines each command with the
latest complete state, then submits the full snapshot with the observed Service
generation. Submission is synchronous at this in-process state boundary so
serialized UI updates cannot overwrite one another; the controller still rejects
non-Active or stale-generation input and neutralizes on Stop. Android-only UI
hides Windows/DSU receiver controls, does not fabricate IMU motion, and does not
claim a Route or remote transport.

ANDROID-A01D3 attaches the permission-free touch surface as a third independent
Service-owned input lifecycle. The shared Vue surface tracks at most five stable
contact slots, normalizes coordinates and pressure to fixed integers, coalesces
MOVE updates to one browser animation frame, and submits a complete contact set.
A closed 160-character grammar crosses the WebView boundary; Java revalidates
canonical integers, count, uniqueness and bounds before the Service advances its
sequence under the active generation. Stop, cancel, blur, hidden-document and
unmount paths release all contacts. This local input state does not claim that
the existing JNI packet Source, authenticated Route or Windows Precision
Touchpad Sink is connected.

ANDROID-A01D4 projects the two existing Service-owned audio slots into the same
shared UI. Microphone Source and speaker Sink remain independent closed
`capability_control` actions; their DTO reports lifecycle generation, actual
platform-negotiated format, processed frames, permission readiness and bounded
problem code. Start is posted back to the Activity main thread so Android's
existing system permission flow remains platform-owned; Stop uses the existing
Service commands. This slice changes no Manifest declaration and retains the
native audio controls as a rollback fallback pending physical UI parity.

ANDROID-A01D5 makes the shared local-capability checkbox mutable on Android,
but keeps its meaning narrow: it is session-local publication intent owned by
the Node Service. Eligibility is derived from the current hardware-backed
catalog and requires an attached Adapter, present hardware and at least one
Source Port. A Sink, Control-only capability or foreign Node ID cannot be
selected. An unattached capability cannot be selected; A01E therefore makes
camera eligible only after attaching its Adapter. Selection does not start hardware, grant permission,
authorize a Route or claim that a peer catalog has received the advertisement.
The Android host now derives a separate local publication catalog from that
selection before any future transport encoding. It retains Sink/Control-only
Capabilities, removes unselected Source-bearing Capabilities and revalidates
the complete candidate selection transactionally. A selected Source that later
becomes Offline can still be removed, but cannot be selected again while
ineligible. This is a local projection boundary only; Android still has no
authenticated catalog transport.
ADR 1404 gives that future Core projection stable typed identities without
persisting a second catalog database. The persisted Android Node UUID is the
UUIDv5 namespace; typed `adapter/`, `capability/` and `port/` paths over bounded
platform keys are shared by Java and Rust fixed vectors. This defines identity
only and does not infer Adapter readiness, QoS, interoperability or Route
support.
ADR 1405 adds the adjacent pure Android-host projection. A closed versioned DTO
must explicitly state Adapter type/version/attachment and Route modes plus Port
Profile, formats, QoS, clock domain and interoperability. The projector builds
and validates complete Core ownership using ADR 1404 identities; it does not
infer these claims in JNI and performs no transport I/O. The Java Service now
encodes its filtered publication catalog into that schema-v1 JSON boundary and
the Rust host rejects unknown fields, unsupported versions and payloads above
64 KiB before Core projection. This is a local handoff contract; a future
native Runtime owner must still invoke it and bind publication to an
authenticated Session.
ADR 1406 introduces that ownership seam without activating it in the Service:
`capyio-android-jni` can create a real `NodeRuntime` from the schema-v1 catalog
and transactionally replace the same local Node while no peers, Sessions or
Routes exist. ADR 1407 then packages the ARM64 library and gives its handle the
same lifetime as `AudioNodeService`; publication intent crosses the native
projection before the Java selection commits. High-rate controller/touch data
does not enter this catalog path, and unchanged catalog JSON is not resent.

ADR 1408 moves the previously separate Android input labs under the unified
Service lifecycle. Touchpad frames retain the ADR 1106 loopback-only ADB
reverse boundary and private JNI Hello/Data/Ack/Close contract. Gamepad
controls and standalone IMU samples share the existing bounded Controller Lab
UDP contract; the operator copies the Windows listener's per-run endpoint and
token into the shared Android UI. Neither worker performs network I/O from a
sensor or UI callback.

ADR 1409 gives the Windows desktop process ownership of the two remaining
operator-started Android receiver helpers. Typed Quick Actions create only the
fixed touchpad or camera ADB reverse mapping and launch only the corresponding
sibling binary with closed arguments. ADR 1410 supersedes only the touchpad
projection choice: a read-only probe selects `--vhf` when exactly one installed
CapyIO VHF interface exists, otherwise the user-mode synthetic path remains the
default. The lifecycle never installs or mutates a VHF device and never
registers a virtual camera. The protected VHF path reports a typed elevation
requirement instead of attempting automatic privilege escalation.
Sidecar exit, explicit Stop and desktop shutdown are reflected through the
typed action state and clean up the exact owned mapping.

ADR 1418 fixes the control-plane boundary for moving the system-camera
projection out of an elevated desktop. A dedicated owner-scoped named pipe
admits only versioned Status/Start/Stop operations and cannot carry paths,
executables, arguments, object names or network endpoints. Its independent
runtime owns one fixed camera projection lifecycle, rolls back partial Start
and reports unexpected exit. This foundation is not yet a Broker executable or
desktop integration, so it does not change the current elevation requirement.

ANDROID-A01E composes the existing headless Camera2/MediaCodec service into the
unified APK behind a narrow facade. The unexported camera foreground service,
not the Activity or WebView, owns capture, encoding, bounded export and cleanup.
The catalog maps that lifecycle to one `capyio.video.frames/1` Source using the
private CAVC access-unit format. The shared action admits only lifecycle-valid
start/stop/retry, and Start returns to the Activity for Android-owned CAMERA and
notification consent. Capture lifecycle, publication intent and a Route remain
three separate states.

The physical Android WebView exposed a `file:`-origin CORS failure for the
packaged ES module and stylesheet. The host now uses a fixed
`https://appassets.androidplatform.net/capyio-ui/` origin served solely by an
APK-asset `WebViewClient`; file/content access is disabled and every other
scheme, host, port or path returns a denied response.

ANDROID-A01H attaches the catalogued haptics Sink to a Service-owned default
Android vibrator. One generation-bound Adapter maps the maximum of the portable
dual-rumble magnitudes onto the single actuator while retaining the contract's
10-second duration bound. It accepts commands only for the current Active
generation; Stop cancels the platform effect. The shared action controls this
local lifecycle and emits one short confirmation pulse on Start. No feedback
Route, peer authority or dual-motor fidelity is inferred.

ANDROID-A01E2 moves the retained lab's bounded Camera2 inventory and AVC quality
selection into the shared presentation. The Android bridge returns a schema-1
settings DTO and admits only an exact cached source key, automatic selection or
one of three quality names. Inventory requires existing CAMERA consent and a
visible refresh; all mutations require a stopped capture. The service receives
an immutable copy on Start, so a running camera/encoder cannot be reconfigured
through the WebView.

ADR 0044 subsequently adds the approved network permission and a separate
dependency-free LAN codec/socket worker boundary. ADRs 0046 and 0047 now attach
the speaker Sink and microphone Source respectively: `AudioTrack` consumes the
bounded receive queue, while `AudioRecord` feeds the bounded packetizer/send
queue. Network I/O remains on dedicated media workers rather than either audio
worker. The Java lifecycle DTO remains neither Rust memory nor a wire layout.

The 001D1 reference backend is direction-neutral and `AdapterManaged`. One
endpoint is pre-bound to the control-approved Session, directed Route, Stream,
epoch and exact selected audio specification, then accepts only canonical
versioned UDP fragments from one explicit peer. Rust provides bounded
reassembly and metrics; Android mirrors the fixed codec and worker socket. This
is a conformance/reference boundary, not the Runtime-owned product composition
or a production transport.

ADR 0045 adds the bounded Android media-worker composition around that
reference: exact PCM packetization, count/byte bounded queues, bounded
reassembly and one-shot sender/receiver threads. Queue pressure is explicit and
the next accepted packet carries discontinuity after a drop. Every component
must share the same binding before start. These workers still stop on either
side of the platform Adapters; `AudioRecord` and `AudioTrack` integration is
owned by the later independent speaker/microphone switching slices.

ADR 0046 connects the speaker half without changing that callback rule. A
build-time closed lab configuration creates the Android UDP receiver; complete
packets cross a bounded queue into a separate non-blocking PCM Sink worker and
then `AudioTrack`. On Windows the protocol-neutral render-ring reader is an
internal platform crate shared by the Audio Share rollback Broker and a new
native Broker. The APO and driver still contain no network or packet logic.
Controlled physical acceptance now proves both a deterministic native tone and
ordinary Windows application playback through this composition. The Windows
service has a separate native launch mode whose readiness record proves only
the Broker/ring boundary; UDP does not provide receiver-presence evidence, so
Android packet/render counters remain the physical acceptance source.

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

The separately packaged Controller Lab is a narrow physical-input spike while
the unified Android Node host is still in flight. Its Activity owns only a
visible touch surface and registers accelerometer/gyroscope callbacks while the
foreground lab stream is active. Sensor callbacks copy finite values into one
bounded snapshot and never perform network or UI work. A dedicated sender emits
a closed complete-state version-1 UDP frame; it does not mirror Rust memory or
claim a StandardPort production transport. The desktop host validates and maps
that frame into independent `GamepadState` and IMU contracts, then feeds the
existing loopback-only DSU Projection. This lab boundary is designed to move
behind the common Android module registry without changing portable contracts.
ANDROID-A01B1 establishes that registry and records the controller module's IMU
Source, gamepad Source and haptics Sink as unattached entries. The standalone
lab remains the lifecycle owner until a later slice attaches its façade to the
service and passes equivalent foreground-loss and neutral-state checks.

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

Gates 0–3 remain the domain, Mock UI and mock Sidecar foundation. Later slices
now include controlled Windows virtual-audio evidence, physical 001C Android
endpoint/lifecycle evidence, the bounded 001D1/001D2 native LAN wire and 001E
Windows-application-to-Android Speaker evidence. Native microphone switching,
other hardware Adapters, production security/distribution and performance
claims remain on the roadmap.
