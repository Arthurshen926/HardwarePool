# CapyIO Protocol v1

## Purpose

`capyio.v1` represents Node identity/catalogs, Sessions, Route intent/status,
authorization and structured Problems. It is independent of sockets and data
planes. Canonical definitions live under `protocol/proto/capyio/v1/` and are
compiled with vendored `protoc`.

## Versioning

Each Envelope has protocol major/minor, typed message ID, optional Session ID
and one payload. Major mismatch is rejected without an explicit compatibility
implementation. Minor evolution appends optional fields. Removed field numbers
are reserved forever within v1. Profile and Adapter-control versions negotiate
independently.

Initial envelope constants remain `1.0`; the pre-alpha package rename from
`hardwarepool.v1` to `capyio.v1` is intentionally breaking and recorded in ADR
0009/0011. No released external consumer exists.

## Identifiers

Node, AdapterInstance, Capability, Port, Route, Session, Message and Problem IDs
are UUID strings at the wire boundary. Type context is validated; equal storage
shape does not make IDs substitutable.

## Catalog messages

- `Hello`: Node identity and supported protocol versions;
- `CatalogSnapshot`: Node metadata, Adapter instances and Capability/Port
  catalog;
- catalog updates may replace an Adapter-owned subset after restart;
- both peers send catalogs; neither is structurally provider-only.

Protocol v1 appends `CAPABILITY_CLASS_TOUCHPAD = 16` without changing existing
enum numbers. It identifies an Adapter-owned touchpad Capability in catalogs;
continuous `capyio.input.touchpad-frames/1` data remains outside Protobuf
control envelopes.

## Route control

- `OpenSessionRequest/Response`;
- `RouteDescriptor`: Source/Sink Port references, backend, compatible and
  selected format/QoS, authorization, epoch, state and diagnostics;
- `RouteCommand`: start/stop intent in the current foundation envelope;
- `RouteStatus`: selected values, state and diagnostic reference;
- `ProblemDescriptor`: structured, typed diagnostic context.

Prepare and authorization transitions currently execute through the local
Runtime/Adapter-control seam. Adding them to the Node-to-Node envelope is later
protocol work and must append fields/messages rather than reusing numbers.

One Route is one direction. Duplex behavior uses multiple Routes.

## Transport separation

Node control requires ordered reliable delivery, mutual authentication,
confidentiality/integrity, replay/downgrade defense, bounded messages and
identity binding before production use. The foundation selects no production
transport and is unsafe for untrusted networks.

High-rate audio/video/sensor data never travels in an Envelope. A Route backend
selects CapyDataPlane, AdapterManaged, LocalPipeline or ExternalProtocol.

Remote touchpad packet v1 is a private `AdapterManaged` data record, not a
Protobuf Envelope or sidecar message. Its surrounding transport must bind an
authenticated/authorized Route and Stream before decoding; the packet codec
itself opens no transport and supplies no trust policy. The Adapter receiver
adds bounded per-session sequence, fixed-window rate and active-idle guards,
but those do not replace authenticated transport replay defense or admission.
The private Adapter ingress additionally binds a fixed-capacity queue to a
current Runtime-owned Core Route snapshot and rechecks its Active,
authorization/expiry and epoch state; this remains process-local lifecycle
validation, not a public wire or bearer credential.

## Sidecar protocol

Sidecar control is a separate JSON-RPC 2.0/NDJSON request/response contract on
stdin/stdout. Gate 3 implements Adapter initialize/probe/catalog/health/shutdown
and Route prepare/start/stop/status. Normal logs go to stderr. The Host is
sequential and has explicit `Running`, `Poisoned` and `Stopped` states; it does
not multiplex concurrent requests.

`route.prepare` carries bounded Route control metadata: Route ID, Source/Sink
PortRefs, Profile, selected format/QoS, backend, epoch, optional data-endpoint
descriptor and optional Adapter configuration. Start/stop/status use generic
bounded request/results. These objects negotiate lifecycle and endpoint
metadata only; continuous media or sensor samples are forbidden.

The Mock Source may attach one finite, low-frequency test sample as a
Mock-private extension to its start response. `SmokeSample` is not part of the
generic Adapter SDK/Host API, and generic consumers receive only the
`RouteStartResult` acknowledgement. Unsolicited Adapter events remain future
control work.

The stdout reader rejects a control line as soon as it exceeds 64 KiB, including
a newline-free stream, before its buffer can grow past the limit. Stderr uses a
separate 2 KiB policy: retain a bounded UTF-8 prefix plus a truncation marker,
then drain through the newline/EOF so the next diagnostic stays aligned. At
most 8 stdout lines and 128 stderr lines are retained/buffered; the SDK permits
at most 64 pending correlation IDs. The default response deadline is five
seconds.

Timeout, unexpected response ID, malformed/oversized response or unexpected
stdout closure is terminal for the sequential channel. The Host becomes
`Poisoned`, closes stdin, terminates and reaps the child while retaining bounded
stderr, and rejects every later request. It never attempts to consume a late
response as a later request's result. A well-formed JSON-RPC error response is a
request failure and does not itself desynchronize the channel.

## Parser limits

The Rust envelope codec rejects messages larger than 1 MiB before decode. Core
then validates required IDs, enum values, catalog ownership and Port/Profile
semantics. Descriptor-count and metadata-string limits are still required
before an untrusted production transport is enabled. Sidecar control uses the
smaller dedicated stdout/stderr limits and terminal desynchronization policy
above.

## Error behavior

Zero/unknown enums, invalid UUIDs, missing semantic fields, unsupported Profile
majors and incompatible Route endpoints return typed errors. Unknown optional
Protobuf fields remain forward-compatible. Human messages are not parsed.

## Compatibility tests

- Core ↔ Protobuf catalog round trip;
- Envelope binary round trip and unsupported major;
- Capability/Port/Route/Problem round trips;
- malformed UUID and missing fields;
- unknown/zero enum values;
- size boundaries.

Unknown-field compatibility fixtures and golden wire fixtures remain required
before external protocol consumption.

Diagnostic/UI JSON snapshots are not automatically public protocol schemas.
