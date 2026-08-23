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
- `NodeCatalog`: Node metadata, Adapter instances and Capability/Port catalog;
- catalog updates may replace an Adapter-owned subset after restart;
- both peers send catalogs; neither is structurally provider-only.

## Route control

- `OpenSessionRequest/Response`;
- `RouteRequest`: Source/Sink Port references, backend, format/QoS intent;
- `RouteAuthorization`: scope and lease result;
- `RouteCommand`: prepare/start/stop/recover as supported;
- `RouteStatus`: selected values, state and diagnostic reference;
- `ProblemReport` and heartbeat.

One Route is one direction. Duplex behavior uses multiple Routes.

## Transport separation

Node control requires ordered reliable delivery, mutual authentication,
confidentiality/integrity, replay/downgrade defense, bounded messages and
identity binding before production use. The foundation selects no production
transport and is unsafe for untrusted networks.

High-rate audio/video/sensor data never travels in an Envelope. A Route backend
selects CapyDataPlane, AdapterManaged, LocalPipeline or ExternalProtocol.

## Sidecar protocol

Sidecar control is a separate versioned NDJSON request/response contract on
stdin/stdout. It includes Adapter initialize/probe/catalog/health/shutdown and
Route prepare/start/stop/status. Normal logs go to stderr. Message size, line
length, pending correlations and retained stderr are bounded.

## Parser limits

The Rust envelope codec rejects messages larger than 1 MiB before decode. Each
binding also enforces descriptor/Port/format/metadata counts and string limits.
Sidecar control uses a smaller dedicated line limit. Data-plane payload bounds
derive from the selected Profile/format.

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
- appended unknown optional field behavior;
- size boundaries;
- golden fixtures before external consumption.

Diagnostic/UI JSON snapshots are not automatically public protocol schemas.
