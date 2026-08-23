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

## Sidecar protocol

Sidecar control is a separate JSON-RPC 2.0/NDJSON request/response contract on
stdin/stdout. Gate 3 implements Adapter initialize/probe/catalog/health/shutdown
and Route prepare/start/stop. Normal logs go to stderr. Lines are limited to 64
KiB, pending correlations to 64, buffered stdout lines to 8 and retained stderr
lines to 128. The foundation Host uses a five-second response deadline.

The Mock Source/Sink return one finite, low-frequency `SmokeSample` when a test
Route starts. It is explicitly a test token, not an audio/video/sensor data
plane. `route.status` and unsolicited Adapter events remain specified future
control methods, not implemented behavior.

## Parser limits

The Rust envelope codec rejects messages larger than 1 MiB before decode. Core
then validates required IDs, enum values, catalog ownership and Port/Profile
semantics. Descriptor-count and metadata-string limits are still required
before an untrusted production transport is enabled. Sidecar control uses the
smaller dedicated line limit above.

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
