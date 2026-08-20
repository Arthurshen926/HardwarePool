# HardwarePool Architecture

> Version: 0.1-bootstrap  
> Status: normative for the bootstrap repository

## 1. Architecture objective

HardwarePool must support a broad long-term device-capability platform without turning the first implementation into an unbounded multi-platform project. The architecture therefore freezes stable semantic boundaries now and leaves transport, codec, operating-system projection, and UI details replaceable.

The system is not “one binary that runs identically everywhere.” It is one repository with:

- a shared, OS-independent Core;
- a shared, versioned wire protocol;
- a shared UI and command contract where practical;
- replaceable transport bindings;
- platform-specific adapters and system projections;
- isolated kernel components only where an OS requires them.

## 2. Architectural principles

1. **Stable semantics, replaceable mechanisms** — Node, Capability, Session, Binding, Projection and Profile semantics are stable; transport and platform implementation can change.
2. **Capabilities are independent** — microphone and speaker are separate resources even when used together.
3. **Profiles own hardware semantics** — Core owns lifecycle; `audio.capture/1` owns audio-specific negotiation.
4. **Least privilege** — authorization is capability-scoped and time-bound.
5. **Thin kernel** — Windows driver code exposes endpoints and PCM IPC only.
6. **Headless runtime first** — UI controls a runtime; it is not the runtime lifecycle.
7. **Observable failures** — every rejection, state change and stream fault has a structured reason.
8. **Agent-verifiable architecture** — important boundaries are represented by workspace dependencies, tests, linters and directory rules.

## 3. System context

```text
+-------------------+                       +------------------------+
| Android phone     |                       | Windows PC             |
|                   |                       |                        |
| Shared Vue UI     |                       | Shared Vue UI          |
| Tauri/Kotlin host |                       | Tauri desktop host     |
| Android Adapter   |                       | Windows Broker         |
| Shared Rust Core  |<-- control/data ----->| Shared Rust Core       |
| Mic + Speaker     |                       | Driver IPC             |
+-------------------+                       | Virtual Audio Driver   |
                                            | Windows Audio Engine   |
                                            +------------------------+
```

The MVP initially substitutes Fake/Mock Adapters for Android audio, network transport, Broker IPC, and the Windows driver. The contracts remain the same as real implementations are added.

## 4. Layer model

```text
UI / CLI
  - shared views and user intent
  - no platform audio logic

Application host
  - Tauri commands, service lifecycle, configuration
  - translates UI intent to Runtime commands

Runtime
  - peers, sessions, bindings, events, snapshots
  - OS-independent orchestration

Core domain
  - identifiers, capabilities, Profiles, state machines, validation
  - no IO and no platform SDKs

Protocol
  - Protobuf v1 messages and Core conversion
  - does not choose a socket implementation

Transport bindings
  - reliable control channel
  - real-time audio frame channel
  - future discovery channel

Platform adapters
  - Android capture/render
  - desktop user-mode audio integration
  - permissions, focus, routing, lifecycle

Projection adapters
  - Windows virtual audio endpoint and Broker IPC
  - future PipeWire/CoreAudio/other projections
```

## 5. Repository module graph

```text
hardwarepool-core
   ^
   +--- hardwarepool-audio
   +--- hardwarepool-protocol
   +--- hardwarepool-runtime
           ^
           +--- hardwarepool-testkit
           +--- hardwarepool-node
           +--- hardwarepool-gui (Tauri backend)

Windows driver: separate future build system; must not depend on Rust Core.
Android native host: generated Tauri/Kotlin project; calls shared Runtime through a narrow host API.
```

Dependency rules:

- Core cannot depend on Runtime, Protocol, Tauri or platform crates.
- Protocol may depend on Core for conversion, but Core cannot depend on Protobuf.
- Runtime may depend on Core but not on Tauri, WDK or Android SDK.
- Testkit may depend on Core and Runtime.
- UI backend may depend on Runtime and Testkit during bootstrap.
- Driver and platform adapters communicate through explicit contracts, not internal Core types serialized by memory layout.

## 6. Core domain model

### 6.1 Node

A Node is an authenticated runtime instance. It has:

- `NodeId` — stable UUID within the project identity store;
- display name;
- platform and platform version;
- roles: provider, consumer, duplex or lightweight;
- capability descriptors;
- protocol version support.

### 6.2 Capability

A Capability is an independently authorized resource. Required fields include:

- `CapabilityId`;
- display name;
- `ProfileId` and major version;
- `CapabilityKind`;
- local role (`capture`, `render`, `control`, `compute`);
- stream role (`producer`, `consumer`, `duplex`);
- supported Projection kinds;
- permission requirement;
- availability;
- typed Profile details or opaque extension details.

### 6.3 Session and Binding

A Session is the control relationship between a local and remote node. It does not imply that all remote hardware is authorized.

A Binding is the per-capability lifecycle:

```text
Requested
  -> Authorized
  -> Negotiated
  -> Starting
  -> Active
  -> Stopping
  -> Stopped

Any non-terminal phase may become Rejected, Offline or Failed.
```

The overall Session can be ready while one binding is active and another is stopped. This is how microphone and speaker remain independent.

### 6.4 Projection

A Projection connects a remote Capability Binding to a local representation:

- `application_stream`;
- `system_capture_endpoint`;
- `system_render_endpoint`;
- future virtual camera, HID, display or compute service.

Projection ID is distinct from Capability ID because one remote ability may be projected differently on different consumers.

### 6.5 Bundle

A bundle is metadata containing member Capability IDs and bundle-level compatibility information. For `audio.duplex_bundle/1`, it may advertise a shared acoustic environment or AEC relationship, but permissions and state remain on the two member capabilities.

## 7. Audio architecture

### 7.1 Data paths

Speaker path:

```text
Windows Audio Engine
 -> virtual render endpoint
 -> driver render ring
 -> Windows Broker
 -> encoder / packetizer / transport
 -> Android jitter buffer / resampler
 -> Android render adapter
```

Microphone path:

```text
Android capture adapter
 -> processor / packetizer / transport
 -> Windows jitter buffer / resampler
 -> Windows Broker
 -> driver capture ring
 -> virtual capture endpoint
 -> Windows Audio Engine
```

### 7.2 Clock domains

Windows and Android audio hardware run on independent clocks. Every stream therefore has:

- source clock-domain ID;
- monotonic source timestamp;
- first-sample index;
- fixed format for the negotiated epoch;
- receiver buffer target;
- drift estimator and dynamic resampler in user mode.

The driver never owns network-clock recovery.

### 7.3 Real-time thread policy

Audio callbacks use preallocated fixed-capacity queues. They may copy frames and update lock-free/low-contention counters, but must not perform socket operations, codec reconfiguration, JSON/Protobuf parsing, logging, disk IO, UI calls, or unbounded allocation.

## 8. Protocol architecture

Protobuf defines the semantic control messages. A transport binding carries those messages over a reliable authenticated channel. Real-time audio frames use a compact frame header and binary payload, which may be carried over UDP, QUIC datagrams, RTP/AOO, or another selected binding.

The protocol has three version axes:

1. envelope protocol major/minor;
2. capability Profile name/major;
3. transport-binding version.

A newer minor version may append optional fields. A newer major version requires explicit negotiation or rejection.

## 9. Runtime architecture

The shared Runtime owns:

- peer registry;
- session registry;
- commands that mutate Core state machines;
- structured event sequence;
- snapshots for UI/CLI;
- future host-capability ports.

Runtime commands are synchronous in the bootstrap implementation to make state deterministic. Async networking and platform work will later submit results back through command/event boundaries rather than mutating Core state from arbitrary callbacks.

Recommended future boundary:

```text
Host Adapter async work
 -> opaque operation ID
 -> completion event
 -> Runtime applies deterministic state transition
```

## 10. UI architecture

The Vue UI depends on a TypeScript `HardwarePoolApi` interface:

```text
getSnapshot()
setProjection(capabilityId, desiredActive)
resetDemo()
```

Two implementations currently exist conceptually:

- `TauriHardwarePoolApi` invokes Rust commands;
- `BrowserMockHardwarePoolApi` runs deterministic local state.

UI components know capability and view-state DTOs, not Windows/Android APIs. Platform-specific controls are added through explicit feature flags and commands.

## 11. Windows architecture

### 11.1 Process boundary

```text
hardwarepool-gui / service controller
             |
Windows Broker process/service
             |
validated fixed IPC protocol
             |
HardwarePoolAudio.sys
             |
Windows Audio Engine
```

### 11.2 Driver responsibilities

Allowed:

- enumerate one render and one capture endpoint;
- negotiate supported PCM formats with Windows Audio Engine;
- transfer PCM through fixed bounded rings;
- report availability and counters through minimal IOCTL/shared-memory contract;
- return silence/drop output safely when Broker is unavailable.

Forbidden:

- sockets and DNS;
- TLS/Noise/pairing;
- Protobuf or JSON;
- Opus/AOO/WebRTC;
- device discovery;
- automatic reconnect;
- user settings and UI;
- dynamic dependency loading.

The initial driver is expected to begin as a separately licensed derivative or clean implementation informed by Microsoft SysVAD. No SysVAD code is included in this bootstrap archive.

## 12. Android architecture

The Android host consists of:

- shared Vue UI in Android System WebView through Tauri;
- generated Android project;
- Kotlin plugin for permissions, foreground-service lifecycle, notifications, audio focus and routing;
- Rust Runtime and protocol library;
- an audio Adapter selected after an Audio Lab spike (CPAL/AAudio, Oboe through FFI, or native Kotlin APIs).

The Activity is not the authority for an active session. A foreground service owns the active microphone/render lifecycle and posts state back to the Runtime.

## 13. Linux and macOS evolution

The Core, Protocol, Runtime, UI contract and testkit are shared. Future system projection adapters are separate:

- Linux: PipeWire/PulseAudio-facing virtual source/sink or session manager integration;
- macOS: Core Audio virtual device or approved modern extension model;
- no platform adapter may claim support until it has real CI and hardware test evidence.

## 14. Security architecture

Trust boundaries:

1. unpaired network peer;
2. paired remote Node;
3. user-mode Runtime/Broker;
4. platform host APIs;
5. kernel driver;
6. UI/WebView.

Every boundary validates length, version, identity, capability scope and lifecycle. A paired Node is not trusted to use every capability. The Broker validates all network and protocol data before converting it into the small PCM/IPC contract consumed by the driver.

## 15. Observability

Every Runtime state mutation produces a structured event with:

- monotonic event sequence;
- timestamp source later supplied by host;
- node/session/capability/projection IDs;
- previous and next state;
- reason/error code;
- optional sanitized metrics.

Audio metrics are per stream:

- packets/frames/samples;
- lost, duplicate and reordered frames;
- jitter estimate;
- buffer fill and target;
- underrun/overrun;
- source/receiver clock ratio;
- encode/decode/resample time;
- estimated end-to-end latency.

## 16. Configuration and persistence

Bootstrap uses in-memory state and deterministic fixtures. Production persistence will require:

- node identity and key store;
- peer trust records;
- user preferences;
- endpoint slot bindings;
- diagnostics retention policy.

Secrets must use platform-protected storage. Configuration file format must not be treated as a key store.

## 17. Compatibility and evolution

- Public Core types are not automatically wire types.
- Protobuf field numbers are stable within v1.
- Profile major versions define breaking semantic changes.
- Unknown optional fields are ignored by Protobuf decoders; unknown required semantics must be rejected at validation.
- Transport Adapter changes do not alter Core state transitions.
- Platform-specific quirks are represented as capability constraints or Adapter behavior, not global exceptions inside Core.

## 18. Initial implementation status

Implemented in this bootstrap:

- Core identifiers, capabilities, audio details and lifecycle state machine;
- Runtime peer/session registry, command methods, events and snapshots;
- Protobuf v1 definitions and conversion layer;
- deterministic testkit and CLI flow;
- Vue/Tauri UI with browser Mock Backend;
- CI, Agent and documentation structure.

Not implemented:

- real-time audio engine and queues;
- network transports;
- Android native Adapter;
- Windows Broker IPC and driver;
- production identity, pairing and encryption.
