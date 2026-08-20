# HardwarePool Protocol v1

## 1. Purpose

The Protocol represents Node identity metadata, Capability descriptors, session intent, Projection commands and structured errors. It is separate from any particular socket, signaling library or real-time audio implementation.

Canonical definitions live in `protocol/proto/hardwarepool/v1/` and are compiled by `hardwarepool-protocol` with a vendored `protoc` binary.

## 2. Version model

Every envelope contains:

- `protocol_major`;
- `protocol_minor`;
- `message_id`;
- optional `session_id`;
- one typed payload.

Rules:

- major mismatch is rejected unless an explicit compatibility implementation exists;
- minor versions are backward-compatible through appended optional fields;
- Protobuf field numbers are never reused;
- Profile version is negotiated independently of envelope version;
- transport version is negotiated by its Adapter.

Initial constants:

```text
protocol_major = 1
protocol_minor = 0
```

## 3. Identifiers

All v1 identifiers are UUID strings at the wire boundary:

- Node ID;
- Capability ID;
- Session ID;
- Binding ID;
- Projection ID;
- Message ID.

Receivers validate UUID syntax and type context. A Capability ID cannot be substituted where a Projection ID is expected merely because both are strings.

## 4. Control messages

Initial payloads:

- `Hello` — protocol support and Node descriptor;
- `CapabilityList` — current Capability descriptors;
- `OpenSessionRequest` / `OpenSessionResponse`;
- `ProjectionRequest` — request a specific mapping for one Capability;
- `ProjectionAuthorization` — provider grant/reject with lease;
- `ProjectionNegotiation` — selected Profile configuration;
- `ProjectionCommand` — start, stop, suspend or resume;
- `ProjectionStatus` — resulting binding/projection state;
- `StreamStats` — sanitized per-stream metrics;
- `Error` — stable machine-readable code and human detail.

The bootstrap `.proto` defines these initial message families. Runtime tests currently exercise descriptors and projection lifecycles primarily through direct Rust APIs; transport wiring remains deferred.

## 5. Capability descriptor

A descriptor contains:

```text
id
display_name
profile { name, major }
kind
local_role
stream_role
supported_projections[]
permission_requirement
availability
details: oneof {
  audio
  audio_bundle
  opaque
}
```

`direction: input/output` is intentionally absent because it is ambiguous across hardware, node and OS-device perspectives.

## 6. Audio descriptor

Audio details contain:

- supported formats;
- supported QoS modes;
- AEC/NS/AGC support;
- volume/mute control flags;

A duplex bundle is a separate relationship descriptor that references independent capture and render Capability IDs. It does not carry or merge their permission state.

Selected stream configuration belongs to session negotiation, not the static descriptor.

## 7. Transport separation

### Reliable control channel

Required properties:

- ordered reliable delivery;
- mutual authentication;
- confidentiality and integrity;
- replay and downgrade resistance;
- bounded message size;
- connection/session identity binding.

Candidate mechanisms may include QUIC streams, WebSocket over mutually authenticated TLS, or a Noise-based channel. The bootstrap does not select one.

### Real-time audio data channel

Required semantics:

- compact binary frame header;
- per-stream sequence and sample index;
- optional codec payload;
- packet loss and reordering visibility;
- bounded receiver buffering;
- authenticated encryption in production.

Candidate mechanisms include a reference UDP/PCM Adapter for the local lab and a production Adapter based on AOO/RTP/QUIC after evaluation.

## 8. Message size and parser limits

The Rust bootstrap codec already rejects a control envelope larger than 1 MiB and validates message/session UUIDs plus the required payload. Before production use, each binding must additionally enforce:

- a potentially smaller transport-specific control-envelope limit;
- maximum descriptor count and string length;
- maximum audio format count;
- maximum metadata/opaque payload length;
- maximum audio frame payload based on negotiated format;
- per-peer rate limits.

Decoded PCM frame semantics, reorder-window behavior and drift estimation are defined in `docs/DATA_PLANE.md` and implemented by `hardwarepool-audio`.

The Windows kernel driver never receives these messages.

## 9. Error model

Errors have:

- stable code;
- category;
- retryable flag;
- human-readable detail;
- related node/session/capability/projection IDs when available;
- optional sanitized context.

Suggested categories:

```text
protocol
identity
authorization
capability
negotiation
lifecycle
transport
platform
audio
driver
internal
```

Human text is not parsed for behavior.

## 10. Compatibility tests

Required test classes:

- Core → Protobuf → Core round trip;
- Protobuf encode/decode round trip;
- malformed UUID;
- unknown enum value;
- unsupported Profile major;
- missing required semantic fields;
- appended unknown optional field;
- maximum-size boundary cases;
- golden binary fixtures after the protocol becomes externally consumed.

## 11. Debug JSON

Core and Runtime snapshots derive `serde` representations for diagnostics and UI. These JSON shapes are not automatically the public wire protocol. Public JSON endpoints require their own schema and version.
