# CapyIO Domain Model

> Status: normative for Profile and Route-independent Core semantics.

## Node

A Node is one logical CapyIO Runtime instance. It has a `NodeId`, display name,
platform, runtime version, supported protocol versions, online state, Adapter
instances and Capabilities. It has no global role.

## AdapterInstance

An AdapterInstance connects the Runtime to hardware, an OS API, a sidecar,
virtual device, Panel, Recorder or external system.

Required properties include ID, Adapter type/version, deployment mode, state,
health, owned Capability IDs and supported Route modes. Deployment modes are
`InProcess`, `Sidecar`, `ExternalService` and `DriverBacked`.

## Capability

A Capability is a resource meaningful to users, such as a microphone, speaker,
camera, display, keyboard, gamepad, IMU, recorder or preview Panel. It has an ID,
Adapter owner, class, display name, availability, permission requirement,
metadata and one or more Ports.

Core classes are deliberately broad: `Microphone`, `Speaker`, `Camera`,
`Display`, `Keyboard`, `Pointer`, `Touchscreen`, `Touchpad`, `Gamepad`, `Imu`,
`Gnss`, `SensorSuite`, `Haptics`, `Recorder`, `Panel`, `Bridge`, and `Custom`.

## Port

A Port is a typed connection endpoint.

```text
Source   produces data
Sink     consumes data
Control  receives control commands
```

Each Port has an ID, owning Capability, display name, direction, Profile ID,
optional Schema/format descriptors, QoS modes, clock domain, availability,
permission requirement and interoperability mode.

`Control` Ports do not substitute for data feedback. A haptics feedback stream,
for example, is a typed Sink rather than an unstructured control escape hatch.

## Profile and format

Profile identifies semantic meaning and version. Format descriptors narrow an
instance to PCM16, Opus, H.264, JPEG, a Protobuf schema, and so on. Transport is
not part of Profile identity.

Compatibility requires equal supported Profile name/major and a mutually
selectable format/QoS, unless a named Converter creates an explicit intermediate
Port. The foundation implements same-Profile validation; Converter execution is
future work.

## Route

A Route connects one `Source PortRef` to one `Sink PortRef` and owns:

- backend (`CapyDataPlane`, `AdapterManaged`, `LocalPipeline`,
  `ExternalProtocol`);
- selected format and QoS;
- state and diagnostics;
- authorization state;
- an epoch that changes after stale/disconnected data is invalidated.

Route states are `Draft`, `Prepared`, `Starting`, `Active`, `Stopping`,
`Stopped`, `Failed`, and `Offline`. State transitions are deterministic and
independent per Route.

An owner may renew an unexpired finite authorization on a Starting or Active
Route (ADR 1505). Renewal strictly extends its expiry without changing its epoch,
does not renew Port leases, and cannot revive a stopped, denied or expired Route.
An authenticated peer request is not itself local authorization to renew.

Route creation validates both endpoint Adapters as well as the Ports. Each
endpoint Adapter must own its Capability and advertise the selected backend.
The foundation applies these minimum backend/interoperability rules:

- `CapyDataPlane` connects two `StandardPort` endpoints;
- `AdapterManaged` connects two `AdapterManaged` endpoints whose Adapters
  explicitly advertise that backend; this declaration is not a claim of
  interoperability with other Adapter contracts;
- `LocalPipeline` connects two co-located `StandardPort` endpoints;
- `ExternalProtocol` may terminate either a matched `StandardPort` pair or a
  matched `AdapterManaged` pair, but both endpoint Adapters must explicitly
  advertise the external backend.

When an Adapter catalog replacement removes an endpoint or changes its
direction, Profile, selected format/QoS support, interoperability contract or
backend support, the Runtime moves only dependent Routes to `Offline`, attaches
a structured Problem and immediately advances their epoch. A compatible catalog
return does not restart them: recovery/restart remains an explicit command and
uses a later epoch. Metadata-only changes refresh the catalog without changing
Route lifecycle state.

Adapter/platform completion is staged through Runtime commands. Successful
connection changes `Starting` to `Active`; a transport loss retains a retryable
Problem, changes the Route to `Offline` and advances its epoch. Recovery is
explicit and `begin_start` advances the epoch again, so late samples from the
failed attempt cannot enter the retried stream.

## Session

A Session is the trust, catalog and control relationship between two Nodes. It
does not own a magical duplex stream and does not authorize every Port. Two
opposite Routes may reference one Session while keeping separate authorization,
format, QoS, state, statistics and failure.

A paired peer record is pre-Session trust metadata, not a Session or Capability
grant. It binds one peer Node ID to a TLS server name, certificate fingerprint
and opaque platform secure-storage handle. Discovery may locate that Node, but
only mTLS authentication and the exact Session/Node binding can admit control.
Revocation prevents a discovery candidate from becoming connectable.

## Port lease

A Port lease is owner-authoritative, Route-scoped usage state, separate from
catalog availability and Route authorization. An explicitly configured local
Port may be exclusive or shared up to a fixed Route count. Claims bind Port,
Route, Session and finite expiry; they cannot exceed Route authorization. A
configured Port cannot start a Route without a current claim. Stop, Offline,
catalog invalidation, peer loss or expiry releases only affected claims. A Node
cannot authoritatively report leases owned by another Node.

An authenticated peer may request acquire, renew or release over the control
protocol, but that request is not authorization or policy. It is accepted only
on the Route's existing Ready Session and only by the Runtime that owns the
named Port. Lease duration is relative and bounded; the owner computes expiry
with its own clock.

## Problem

A Problem is a structured diagnostic with stable code, category, severity,
retryability, human message, sanitized technical detail and optional related
Node, Adapter and Route IDs. Human text is never parsed for behavior.

## Non-Core mechanisms

System projections, Panels, Recorders, ROS topics, USB/IP devices and virtual
devices are expressed as Adapter-owned Capabilities and Ports. UI may group them
by mechanism, but Core does not add a distinct lifecycle hierarchy for each.
