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
- `capyio-input` defines normalized pointer/generic-touch/keyboard/gamepad/
  haptics semantics plus physical five-contact touchpad frames and
  allocation-free fail-safe epoch/sequence guards. It does not inject input,
  implement DSU/VIIPER/HID/USB-IP or depend on a platform SDK.
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

For Windows Precision Touchpad projection, ADR 0041 selects a runtime-loaded
user-mode synthetic touchpad as the first feasibility path. The platform
Adapter, not Core or `capyio-input`, owns `user32.dll` discovery, native
structure translation and device-handle cleanup. API availability alone is not
a gesture-compatibility claim. VHF remains a separately reviewed fallback and
still requires a minimal driver boundary, isolated deployment plan and physical
acceptance evidence.

Physical acceptance subsequently showed complete tap, one-finger and
two-finger behavior but no configured Shell action from accepted three- or
four-contact input. ADR 0048 therefore activates a compile-only VHF fallback.
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

ADR 0049 composes those records with the packet receiver and an injected
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
physical lab only, ADR 0047 supplies an ADB-paired reverse tunnel to a Windows
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

ADR 0044 adds a private AdapterManaged packet between the Android mapper and
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

## 12. Android boundary

Android platform code owns permissions, microphone indicators, foreground
service, audio focus/routing and hardware APIs. Activity/window lifecycle is not
session lifecycle. The current foundation contains no permission or service
implementation and makes no hardware claim.

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
