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

## 5. Repository module graph

```text
capyio-core
   ^
   +--- capyio-data-plane
   +--- capyio-audio (shared direction-neutral audio contracts/algorithms)
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

The microphone projection uses two cooperating Windows principals. The
privileged `CapyIOBroker` owns only the fixed global capture ring; an
ordinary-user `capyio-microphone-host` owns the MicYou process and ADR 0039
configuration. Its remote-rejecting owner-scoped named pipe exposes only typed
status/start/stop state. This source slice does not yet install or register the
per-user host at logon, so direct Tauri ownership remains a development
fallback until installer work is completed.

## 12. Android boundary

Android platform code owns permissions, microphone indicators, foreground
service, audio focus/routing and hardware APIs. Activity/window lifecycle is not
session lifecycle. `CAPY-AUDIO-NATIVE-001` targets one CapyIO Android Node
service with independent microphone Source and speaker Sink Adapters behind the
ADR 0041 media seam.

ADR 0043 and `CAPY-AUDIO-NATIVE-001C` now provide the first buildable platform
shell. A non-exported, `START_NOT_STICKY` foreground service owns real
`AudioRecord` and `AudioTrack` handles; the Activity observes a narrow
schema-v1 snapshot and requests independent operations. Each capability has a
separate generation so a late platform completion cannot reverse Stop or fail
the opposite direction. Actual Android parameters are retained rather than
assuming the requested 48 kHz PCM mode was granted.

The shell deliberately has no network permission. Microphone bytes are read on
a bounded/preallocated worker and discarded, while the empty speaker Track
waits for 001D. Java DTOs are not Rust memory or wire layouts. No physical
permission, background, focus, routing, quality or native transport claim is
made until a separately authorized APK/device run.

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
now include controlled Windows virtual-audio evidence and the compile-validated
001C Android audio service shell. Native Android networking, physical APK
evidence, other hardware Adapters, production security/distribution and
performance claims remain on the roadmap.
