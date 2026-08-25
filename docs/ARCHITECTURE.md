# CapyIO Architecture

> Version: 0.3-pre-alpha
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
configuration remain in user mode. Driver build/install/test requires an
isolated VM or dedicated Windows installation.

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
