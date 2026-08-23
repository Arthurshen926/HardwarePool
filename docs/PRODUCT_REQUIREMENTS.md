# CapyIO Product Requirements

> Version: v0.3-pre-alpha
> Status: normative foundation baseline
> Working name: CapyIO — Cross-device I/O Capability Fabric
> License baseline: Apache-2.0

## 1. Product definition

CapyIO connects I/O capabilities already present in phones, tablets, laptops,
desktops and embedded devices. A capability may be projected as a system device
where a platform supports it; otherwise it remains useful through an API,
built-in Panel, standard protocol or recording output.

CapyIO is not remote desktop, game streaming, USB/IP, one universal media
transport, or a single-purpose phone peripheral tool. Those mechanisms and
existing vertical projects may be integrated through Adapters, but do not define
the Core.

The product has two experiences:

- **Quick Actions** for task-oriented one-switch workflows;
- **Workspace / Lab** for Nodes, Capabilities, Ports, Routes, Adapters,
  Problems, Panels, Recorder and Playback.

## 2. Fixed scenario baseline

- **FR-SCEN-001**: A phone/tablet microphone can feed a desktop application and,
  where supported, appear as a system microphone.
- **FR-SCEN-002**: A phone/tablet/laptop speaker can consume audio from another
  Node; the first desktop path may mirror system mix before a dedicated virtual
  render endpoint exists.
- **FR-SCEN-003**: Touch controls, IMU and feedback can form a multi-Port gamepad
  Capability with independent forward input and reverse haptics Routes.
- **FR-SCEN-004**: A camera can feed a preview/API and, where supported, a system
  virtual camera without forcing video mechanisms into Core.
- **FR-SCEN-005**: Screen mirror and extended display are separate workflows;
  mirror precedes OS virtual-display work.
- **FR-SCEN-006**: Keyboard, pointer and touchpad capabilities can target
  platform-appropriate input projections without a fixed device direction.
- **FR-SCEN-007**: Camera, depth, IMU, GNSS, barometer and audio can be routed to
  compute, visualization and multi-stream recording while preserving provenance.
- **FR-SCEN-008**: Multiple devices can compose a temporary workstation in one
  workspace, each contributing only selected capabilities.
- **FR-SCEN-009**: Recorded data and Mock Adapters can support rescue, replay,
  disconnection, jitter, hot-plug and abnormal-sensor testing.

## 3. Domain requirements

### Nodes and sessions

- **FR-NODE-001**: Every Node has a typed stable ID, display name, platform,
  runtime version, protocol support and online state.
- **FR-NODE-002**: A Node has no global provider, consumer, server or client
  product role.
- **FR-NODE-003**: One Node can simultaneously own Source, Sink and Control Ports.
- **FR-NODE-004**: Both peers exchange catalogs; neither side's catalog is
  structurally privileged.
- **FR-NODE-005**: Unknown or offline Nodes are never implicitly authorized.
- **FR-SESSION-001**: A Session represents trust, catalog exchange and control
  between two Nodes, not blanket access to every capability.
- **FR-SESSION-002**: Capability/Route authorization is independently grantable,
  expiring and revocable.
- **FR-SESSION-003**: Reconnect establishes a new data epoch and cannot replay
  stale traffic as current data.

### Adapters and capabilities

- **FR-ADAPTER-001**: Every Capability is owned by one AdapterInstance.
- **FR-ADAPTER-002**: Adapter deployment mode is explicit: InProcess, Sidecar,
  ExternalService or DriverBacked.
- **FR-ADAPTER-003**: Adapter health, state and owned Routes are observable.
- **FR-ADAPTER-004**: A Sidecar crash only fails its owned Capabilities/Routes;
  Core, unrelated Adapters and UI remain available.
- **FR-ADAPTER-005**: Existing projects retain their own data planes behind an
  AdapterManaged Route until a true StandardPort integration exists.
- **FR-ADAPTER-006**: Every distributable Adapter has a versioned manifest with
  entrypoint, platform, permission, integration and license metadata.
- **FR-CAP-001**: Capability is a user-understandable resource with a typed
  class, Adapter owner, availability, permission requirement and metadata.
- **FR-CAP-002**: Each routable Capability owns at least one Port.
- **FR-CAP-003**: Capability has no input/output direction; direction belongs to
  each Port.
- **FR-CAP-004**: Compound resources such as gamepads may own multiple Ports in
  different directions.
- **FR-CAP-005**: Unknown/custom Capability classes remain explicit and do not
  get silently reinterpreted as known classes.
- **FR-CAP-006**: Panels, recorders, projections, bridges and external exports
  are represented as Adapter-owned Capabilities/Ports rather than new Core
  object hierarchies.

### Ports, Profiles and Routes

- **FR-PORT-001**: Port direction is exactly Source, Sink or Control.
- **FR-PORT-002**: A Port declares a versioned Profile, optional Schema/format
  descriptors, QoS modes, clock domain, availability and permission requirement.
- **FR-PORT-003**: Profile describes semantic payload meaning, not an operating
  system API or transport library.
- **FR-PORT-004**: Source timestamps, receive timestamps, sequence, units,
  coordinate frames, accuracy and calibration remain representable when a
  Profile needs them.
- **FR-PORT-005**: Unsupported Profile majors and unknown required semantics are
  rejected explicitly; they are never silently coerced.
- **FR-ROUTE-001**: A Route connects exactly one Source Port to one Sink Port.
- **FR-ROUTE-002**: Route creation rejects same-direction endpoints and
  incompatible Profile majors unless an explicit Converter is selected.
- **FR-ROUTE-003**: Route state is one of Draft, Prepared, Starting, Active,
  Stopping, Stopped, Failed or Offline.
- **FR-ROUTE-004**: Invalid Route transitions return typed, diagnosable errors.
- **FR-ROUTE-005**: Stopping or failing one Route does not mutate an unrelated
  Route.
- **FR-ROUTE-006**: Two opposite-direction Routes between the same Nodes can be
  active simultaneously.
- **FR-ROUTE-007**: Route backend is explicit: CapyDataPlane, AdapterManaged,
  LocalPipeline or ExternalProtocol.
- **FR-ROUTE-008**: Selected format, QoS, authorization state and diagnostics
  belong to the Route and remain independently observable.

### Problems and diagnostics

- **FR-DIAG-001**: Problems carry stable code, category, severity, retryability,
  human message, sanitized technical detail and related object IDs.
- **FR-DIAG-002**: Runtime mutations emit bounded, monotonically sequenced
  structured events.
- **FR-DIAG-003**: Adapter stderr can be retained only through a bounded,
  sanitized diagnostic path.
- **FR-DIAG-004**: Logs never contain raw microphone/video payloads, credentials,
  private keys or unrelated personal data by default.

## 4. Protocol and control requirements

- **FR-PROTO-001**: Node-to-node control uses an explicit major/minor version and
  stable numeric Protobuf fields.
- **FR-PROTO-002**: Removed field numbers are reserved and never reused.
- **FR-PROTO-003**: Control messages cover Hello, Catalog, Session, Route,
  authorization, status, Problem and heartbeat semantics.
- **FR-PROTO-004**: High-frequency media/sensor payloads are not carried inside
  control envelopes or Sidecar JSON-RPC.
- **FR-PROTO-005**: Public wire types are converted to validated Core types;
  internal memory layouts are never wire contracts.
- **FR-PROTO-006**: Zero/unknown enums and unsupported major versions have
  explicit tested behavior.
- **FR-PROTO-007**: Sidecar control is newline-delimited JSON request/response
  messages on stdin/stdout; ordinary logs use stderr.

## 5. User-experience requirements

- **FR-UX-001**: Quick Actions lead with user tasks, not Port/Route internals.
- **FR-UX-002**: Quick Actions are versioned Route Templates and display the
  selected devices, permission state and resulting status.
- **FR-UX-003**: Workspace exposes Connections, Nodes, Capabilities, Ports,
  Routes, Adapters, Problems, Panels and Recorder/Playback navigation.
- **FR-UX-004**: Initial Route Builder uses accessible lists, cards and selectors,
  not a complex node graph.
- **FR-UX-005**: Browser Mock and Tauri Mock use one DTO contract and label all
  simulated state and metrics.
- **FR-UX-006**: One Route can be started/stopped without an implicit change to
  any other Route.
- **FR-UX-007**: Initial Panels are built in; there is no dynamic third-party
  Panel market in the current foundation.

## 6. Platform requirements

- **FR-PLAT-001**: Windows/Linux/macOS prefer isolated Sidecars for large or
  failure-prone integrations.
- **FR-PLAT-002**: Android/iOS may use in-process Adapters or a platform-managed
  service when process restrictions require it.
- **FR-PLAT-003**: UI/window closure does not define Runtime or mobile service
  lifecycle.
- **FR-PLAT-004**: System Projection is preferred where safe and supportable, but
  every Profile may define API/Panel/protocol/recording fallback behavior.
- **FR-PLAT-005**: Android never claims ordinary-app access to global virtual
  microphone/camera/audio devices that the platform cannot expose.
- **FR-PLAT-006**: No microphone capture starts without the platform-required
  visible permission and lifecycle state.

## 7. Non-functional requirements

### Security and privacy

- **NFR-SEC-001**: Production node control/data paths require mutual
  authentication, confidentiality, integrity, replay defense and downgrade
  binding before use on untrusted networks.
- **NFR-SEC-002**: Authorization scope is per Capability/Route, time-bound and
  immediately revocable.
- **NFR-SEC-003**: Untrusted protocol or network parsers never enter a kernel
  driver.
- **NFR-SEC-004**: UI/WebView commands are allow-listed and expose no arbitrary
  shell, filesystem or remote-content power.
- **NFR-SEC-005**: The current insecure/mock foundation is never described as
  production-safe networking.

### Stability and real time

- **NFR-STAB-001**: Disconnect, Adapter failure and Runtime exit do not crash or
  indefinitely block an operating-system audio/input service.
- **NFR-STAB-002**: Operations, events, queues, messages and retained logs have
  explicit bounds.
- **NFR-STAB-003**: Adapter restart updates the catalog and invalidates stale
  Route epochs deterministically.
- **NFR-STAB-004**: Failure and partial availability are explicit; unrelated
  Routes stay usable.
- **NFR-STAB-005**: Soak claims require retained evidence; source generation or
  unit tests do not substitute for hardware results.
- **NFR-RT-001**: Real-time callbacks do not block, perform file/network I/O,
  wait on contended locks or emit ordinary logs.
- **NFR-RT-002**: Real-time data structures use preallocated or fixed-capacity
  storage on callback paths.
- **NFR-RT-003**: Independent clocks use explicit clock domains, timestamps and
  user-mode recovery; kernel drivers do not recover network clocks.

### Maintainability

- **NFR-MAINT-001**: Core, Protocol and Runtime tests run without hardware on
  Windows, Linux and macOS CI.
- **NFR-MAINT-002**: Architecture boundaries are represented in Cargo
  dependencies, tests and offline repository validation.
- **NFR-MAINT-003**: Public protocol/architecture changes have an ADR,
  compatibility note and tests.
- **NFR-MAINT-004**: Third-party integrations record upstream revision, license,
  integration mode, imported paths, modifications and build/runtime risks.
- **NFR-MAINT-005**: Unified `xtask` commands are safe by default and do not
  install drivers, APKs or system components.

## 8. Foundation acceptance (Gates 0–3)

1. User-visible names, crates and Protobuf package use CapyIO.
2. Core contains no NodeRole and models AdapterInstance, Capability, Port,
   Route, Session and Problem.
3. Direction and Profile compatibility tests cover success and rejection.
4. Four deterministic cross-direction Routes can be controlled independently.
5. Protocol and manifest schemas round trip/validate.
6. Adapter Host completes a mock Sidecar lifecycle and isolates child failure.
7. Browser/Tauri Mock UI exposes Quick Actions and Workspace.
8. All available Rust, repository and UI checks pass.
9. No real hardware, APK, driver or production network action occurs.

## 9. Current non-goals

- real MicYou, Audio Share, VCamdroid, SensorServer or VIIPER integration;
- Windows virtual audio/camera/display/HID implementation or installation;
- Android permissions, foreground service, APK install or physical-device test;
- production pairing/encryption, WAN relay, Mesh or NAT traversal;
- unified WebRTC/media/data plane, ROS 2, MCAP, FFmpeg or USB/IP implementation;
- plugin marketplace, cloud service or professional audio/video guarantees.

## 10. Public-alpha proof bar

A future public alpha must demonstrate one UI and one node identity across at
least three Capability Classes, both Android→Windows and Windows→Android
directions, one system Projection, one Panel/API/Recorder output, independent
Route failure, and a single user pairing flow. This is a roadmap target, not a
claim about the current repository.
