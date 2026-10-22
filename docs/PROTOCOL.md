# CapyIO Protocol v1

ADR 1509 defines the Windows local Node API v1 (not the cross-device protocol).
A current-user/session named pipe carries length-prefixed UTF-8 JSON control frames
up to 1 MiB: `{ "version": 1, "command": "get_snapshot", "args": {} }`.
Responses use `{ "Ok": <existing DTO> }` or `{ "Err": <message> }`; the client sends
one byte `1` after reading the complete response before the instance disconnects.
Unknown versions/commands and invalid typed arguments fail before dispatch. The
53 existing Desktop command names and camelCase arguments are preserved in
`host_commands.rs`; only local `host_status` and `host_shutdown` manage process
lifetime. This API never carries media. SECURITY_MODEL.md describes OS identity
checks and limits; existing cross-device Session/Route/lease/epoch rules still apply.

Minor 7 adds Start-only `DisplaySessionControl.automatic` (field 11, default
false). When true, the request selection is a placeholder; the receiver returns
its screen-fitting, decoder-probed final selection with Capabilities. The host
checks its adapter policy and exact capability intersection before Launch;
Launch confirms that exact selection. Missing final selection fails automatic
startup rather than silently treating it as manual. Manual requests keep exact
selection semantics. This is startup selection, not dynamic network adaptation.
Both nodes must be updated to use automatic mode.

The staged display launch adapter also has dedicated `capyio-display-launch/1`
ALPN with `CPYDSP01` route binding. It reuses paired-node TLS certificate pins,
finite grants, Session/Route/Stream/epoch and Source/Sink binding. It carries
bounded 176-byte GameStream launch material and its acknowledgement, not video.
It is not yet advertised by node catalogs or connected to the product workflow.

ADR 1505 adds `DisplayStreamCapabilities`, `DisplayStreamMode` and
`DisplayStreamSelection` for exact H.264 SDR mode/bitrate negotiation. Envelope
64 (`DisplaySessionControl`) carries a fresh operation UUID and, once assigned,
the authorized Route ID/epoch. Start, Capabilities, Launch, Ack, Presented, Stop
and Failed are distinct operations; Ready (8) follows the service's worker-start
acknowledgement and permits the receiver to begin media negotiation. The shared `negotiate_display_settings`
rejects unsupported explicit selections without fallback. Launch carries one
176-byte secret-bearing adapter record; Ack carries exactly 64 bytes. Neither
record may be logged, persisted, included in diagnostics or exposed to WebView.
These are bounded session setup messages over the existing authenticated node
TLS connection, not video. Video remains on the GameStream adapter transport.
Pairing authenticates the peer but does not authorize capture or reception;
the runtime checks local resource consent, Route binding and finite leases.
Presented requires positive receiver frame evidence; a listener or successful
launch acknowledgement must not be represented as delivered video. Both peers
need the coordinated display-workflow update. Legacy display provisioning is a
separate compatibility path, not an automatic fallback for this workflow.

ADR 1503 adds optional `RouteStatus.camera_data` (tag 5): fixed TCP port 38173
and nonzero Stream UUID. Only the Route Sink sends it to its authenticated Source.
The enclosing AdapterManaged `capyio.video.frames/1` Route must be Starting or
Active, select `cavc-avc-access-unit-v1`, and carry a positive bounded epoch and
finite grant. It is mutually exclusive with other data offers. Absent fields
retain existing audio/touchpad behavior. This configures the controlled-lab CAVC
worker, not a production encrypted video transport or proof of frame delivery.

Android's local management consent DTO recognizes `resourceKind: gamepad` with
an exact Capability ID (ADR 1500). This is not a new wire envelope or a data-path
readiness claim. Gamepad Route Start without its admitted data worker reports
Offline with `CAPY.GAMEPAD.DATA_WORKER_UNAVAILABLE`; it never dispatches to the
touchpad/haptics worker or becomes Active from consent alone.

ADR 1499 adds Envelope fields 62/63 for controller settings requests and applied
state. An authenticated peer may request motion-enabled and strength (0..100)
preferences using a nonzero request ID and expected owner revision. Android
returns its persisted applied values/revision and the matching request ID, or a
bounded error and unchanged current values on conflict/failure. These messages
never authorize/start hardware or create a haptics Route. Requests are scoped to
the Ready TLS Session. Both applications require the coordinated 0.5.22 update;
older workers reject these new payloads. Sensor data remains outside control RPC.

ADR 1472 refines touchpad control policy without changing wire fields: an owner
may return a Draft/Pending Route while local consent is outstanding. A caller
must not treat Pending as Authorized or acquire a lease; it may retransmit the
same bound Route after confirmation. Denied remains a distinct terminal decision
for that proposal. Authorization, lease acquisition and data-plane readiness
remain separate stages.

## Purpose

`capyio.v1` represents Node identity/catalogs, Sessions, Route intent/status,
authorization and structured Problems. It is independent of sockets and data
planes. Canonical definitions live under `protocol/proto/capyio/v1/` and are
compiled with vendored `protoc`.

## Versioning

ADR 1497 versions the private Android controller UDP payload independently:
Android 0.5.19 sends version 2 with mandatory `motionAvailable` and
`gamepadMotionEnabled` booleans. Unavailable motion has zero component timestamps;
available samples retain original positive source timestamps, no more than
350 ms older than the packet timestamp. Controls remain usable without motion.
The updated Windows decoder accepts v1 controls but marks sentinel/stale motion
unavailable. It gates DS4 motion separately from independent sensor availability
and stops DSU export on motion loss. This requires paired app updates; older
Windows rejects v2, with no automatic data-version negotiation or v1 downgrade.
It does not change the authentication, authorization or trusted-network scope.

Each Envelope has protocol major/minor, typed message ID, optional Session ID
and one payload. Major mismatch is rejected without an explicit compatibility
implementation. Minor evolution appends optional fields. Removed field numbers
are reserved forever within v1. Profile and Adapter-control versions negotiate
independently.

Envelope constants are now `1.8`. Minor 6 adds `DisplaySessionControl` at field
64; reserved fields are unchanged. Minor 5 adds `ControllerReceiverOffer` at
Envelope field 61 (the reserved 48–60 range is unchanged). An authenticated
Session transports the adapter-managed receiver host, UDP port and random token.
Port zero with an empty token withdraws controller reception while retaining
the peer address for the camera compatibility adapter. Android replaces the
offered host with the actual authenticated socket peer IP; neither discovery nor the
public catalog contains the token. Windows explicitly starts its receiver;
Android only configures its destination and still requires local hardware Start.
Session closure clears that destination. This is compatibility configuration,
not Route authorization/lease negotiation or encrypted UDP; use is limited to
the existing trusted-lab/Tailscale path and requires updated apps on both peers.

Minor 8 appends optional `RouteStatus.secure_audio_data` at field 7
([ADR 1559](adr/1559-finite-secure-mono-audio.md)). This receiver-only offer
requires the exact empty-parameter `pcm-s16le-48000-mono-tls13-v1` selection and
Interactive QoS. It is mutually exclusive with all other data offers. The
dedicated pinned TLS 1.3 channel uses `capyio-audio/1`, `CPYAUD01` Route binding,
and u16 framing of exactly one 1,064-byte canonical mono CPYA packet per record.
Original consent, leases, generation and finite deadlines remain independent
requirements; receipt of an offer is not permission. Unknown transport, mixed
offers, changed selections and replay fail closed without UDP fallback. The
first implementation is a Rust sender/owner API with synthetic graph acceptance,
not an Android AudioRecord or console secure-audio advertisement.

[ADR 1560](adr/1560-android-secure-microphone-binding.md) adds explicit Android
secure microphone opt-in using this same offer and format, without another wire
version or transport. Eligibility advertisement is not capture verification.
JNI packetization uses a relative sample-clock timeline: each complete 480-sample
frame advances sequence by one, sample index by 480 and timestamp by 10,000 us.
These timestamps do not claim absolute device capture time or A/V synchronization
and cannot extend authority. Legacy UDP and secure mono remain distinct choices;
no format/offer fallback is permitted within existing consent.

Minor 4 appends optional
`RouteStatus.native_audio_data` at field 3 (ADR 1494). This metadata names a
nonzero port, Stream UUID and Source/Sink endpoint, with explicit mono
48 kHz S16LE/10 ms preset (value 1) and **trusted-lab UDP v1** transport enums.
ADR 1502 adds stereo 48 kHz S16LE/10 ms preset value 2 without changing mono.
Reciprocal offers must agree on preset and the selected format must exactly
match `pcm-s16le-48000-mono` or `pcm-s16le-48000-stereo`. Older peers reject the
new stereo value; there is no implicit mono fallback or channel conversion. Zero or
unknown enum values fail closed; audio and touchpad offers cannot coexist.
The offer requires a finitely authorized Starting/Active AdapterManaged audio
Route and an Android-compatible epoch. The authenticated sender must own the
named endpoint, and the other endpoint must belong to the local peer. IP comes
from the authenticated control connection, not an offer field. These checks
do not grant hardware permission, verify current leases or secure the UDP data.
Control owners must require explicit lab-transport policy and exact negotiated
format, match both offers' Stream/Route/epoch and recheck live state before
starting workers. Older peers may ignore the optional offer; missing reciprocal
metadata is not permission to use fixed lab identities or report Active.

Minor 3 appends optional `RouteStatus.touchpad_data`
at field 2. It contains a nonzero listener port and fresh Stream UUID, never a
host address or input payload. It is valid only for a finitely authorized,
nonzero-epoch Starting/Active AdapterManaged touchpad Route. The recipient uses
the authenticated control peer's address and pins the same paired identity on
a separate TLS connection (`capyio-touchpad/1`). Missing offers cannot establish
real touchpad readiness. Minor 1 appends owner-authoritative Port
lease request/response payloads. Minor 2 appends `ControlHeartbeat` at Envelope
field 47. It carries a nonzero connection-local sequence and acknowledgement
flag under an authenticated Session ID, but no clock, authorization, catalog or
hardware payload. A receiver echoes the sequence with acknowledgement set;
missing bounded acknowledgements are liveness evidence only. Long-lived link
owners dispatch heartbeat and CatalogSnapshot by payload type, so a complete
catalog update may appear before the acknowledgement without violating wire
order or changing either message's semantics. The pre-alpha package rename from
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

The Rust protocol boundary exposes symmetric complete-catalog encoding and
decoding helpers. Encoding requires a typed Session ID and validated Core
`NodeDescriptor`, converts it to the public wire descriptor and applies the
1 MiB control-envelope bound. Decoding requires a non-empty typed Session ID,
an exact `CatalogSnapshot` payload and a complete `NodeDescriptor`, then
performs the existing wire-to-Core validation before returning a host value.
These are structural/semantic codec operations only: the host must authenticate
the connection, apply replay policy and pass the decoded value to the Runtime's
exact-Session peer-catalog admission method.

The shared control-host composition performs that decoder-to-Runtime handoff
for a caller-supplied admitted Session/peer binding. Its fixed 256-message
recent-ID set only suppresses immediate duplicate delivery; it is not the
transport replay window and does not survive process restart.

For outbound publication, the control host projects the complete local Core
catalog before calling the encoder. Unselected Source-bearing Capabilities are
absent from the wire snapshot, Sink/Control-only Capabilities remain visible,
and Adapter ownership is adjusted and revalidated. The wire schema is unchanged.
The bound host API derives the outbound Session ID from its admitted inbound
peer binding rather than accepting another caller-selected ID.

Protocol v1 appends `CAPABILITY_CLASS_TOUCHPAD = 16` without changing existing
enum numbers. It identifies an Adapter-owned touchpad Capability in catalogs;
continuous `capyio.input.touchpad-frames/1` data remains outside Protobuf
control envelopes.

## Route control

- `OpenSessionRequest/Response`;
- `RouteDescriptor`: Source/Sink Port references, backend, compatible and
  selected format/QoS, authorization, epoch, state and diagnostics;
- `RouteCommand`: session-bound start/stop intent for an existing mirrored
  Route;
- `RouteStatus`: a session-bound pending proposal or current mirrored Route
  state, authorization, selected values and diagnostic reference;
- `ProblemDescriptor`: structured, typed diagnostic context.

ADR 1471 admits these existing messages on the live authenticated control link.
A new peer proposal must be Draft/Pending and resolve to the exact TLS-bound
Session and its two Nodes. Each platform independently authorizes its mirror;
Start additionally requires a live owner-authoritative lease for the local Port.
This transition does not carry payload data or bind a concrete Adapter endpoint.

One Route is one direction. Duplex behavior uses multiple Routes.

## Port lease control

Protocol v1.1 appends `PortLeaseRequest` and `PortLeaseResponse`. A request
names one existing Route and one exact local PortRef and selects acquire, renew
or release. Acquire/renew carry a relative TTL from 1 second through 5 minutes;
release carries zero. The owner derives the absolute expiry from its own clock.
Responses correlate the original Message ID and expose only acquired, renewed,
released, conflict or denied plus an expiry for successful acquire/renew.

ADR 1486 uses these existing renew messages for active touchpad sessions. The
consent grant remains finite (at most eight hours), while each lease is at most
five minutes and bounded by that grant. Renewal does not reauthorize a Route
or change its TLS data binding. Catalogs and heartbeat requests may precede a
correlated response and are dispatched within a fixed message-count bound.

The shared control host accepts these messages only on its fixed admitted
Session/peer binding. The Runtime Session must be Ready, the Route must belong
to that Session and the requested Port must be owned locally. Existing Route
authorization and owner-configured lease policy remain mandatory. The request
cannot create or authorize a Route, select a policy, start hardware or lease a
Port owned by another Node. Recent duplicate suppression is shared with catalog
messages and remains only a bounded host safeguard, not cryptographic replay
protection. No production socket or authentication handshake is implemented by
the protocol codec or control-host slice. ADR 1436 adds a separate mTLS 1.3
transport capable of carrying these Envelopes once paired credentials and an
exact Session/Node binding are supplied by a platform host.

## Transport separation

ADR 1448 requires the platform's paired leaf-certificate fingerprint on both
sides of the TLS transport. Chain verification alone is insufficient; the exact
leaf must match before application binding records are sent or accepted. This
changes host configuration, not the version-1 control wire format.

Node control requires ordered reliable delivery, mutual authentication,
confidentiality/integrity, replay/downgrade defense, bounded messages and
identity binding before production use. The foundation selects no production
credential-enrollment mechanism. The generic control transport now supplies
TLS 1.3, mutual certificate verification, a dedicated ALPN, exact
Session/server/client Node binding and bounded length-prefixed Envelope I/O.
Credential enrollment, secure storage, rotation, revocation, reconnect and
multi-peer listener scheduling remain platform/product work.

ADR 1455 retains the same fixed binding bytes while permitting a paired mTLS
listener to accept a fresh random Session ID proposed by the dialer after TLS
authentication. The listener still requires its preconfigured server/client
Node IDs and acknowledges the exact complete record. This removes out-of-band
Session precoordination; it is not `OpenSessionResponse`, Runtime admission,
Capability authorization or reconnect policy. Active Session-ID uniqueness and
transactional Runtime creation remain host-owned.

High-rate audio/video/sensor data never travels in an Envelope. A Route backend
selects CapyDataPlane, AdapterManaged, LocalPipeline or ExternalProtocol.

For audio, ADR 0041 defines a separate in-process media binding containing the
typed Session, Route, Stream, epoch and exact selected stream specification.
That binding is the control-to-data seam; `AudioMediaPacket` is not a
`capyio.v1` payload or a public network serialization. A future transport must
authenticate and bind the same identities rather than trusting values supplied
by an unauthenticated media peer.

ADR 0042 adds a backend contract beside that seam. It records full-packet,
PCM-payload-only or opaque access plus field fidelity and observable security;
it is neither serialized in `capyio.v1` nor permission to claim a partial
compatibility wire as a StandardPort.

ADR 0044 defines `dev.capyio.audio.lan-lab/1`, a separate versioned UDP golden
wire for transport conformance. It is not an Envelope payload, does not change
`capyio.v1`, and is not a production/public StandardPort. Its header repeats
the control-approved Session/Route/Stream/epoch so a bound endpoint can reject
cross-Route data, but those unauthenticated fields are not proof of peer
identity or authorization.

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

The Rust envelope codec rejects messages larger than 1 MiB before decode. A
CatalogSnapshot additionally permits at most 64 Adapters, 256 Capabilities, 32
Ports per Capability, 32 formats per Port, 64 Capability metadata entries or
format parameters, and 16 protocol versions. Adapter ownership references are
bounded by the Capability limit; QoS and Route-mode enum lists are limited to
16. Every catalog string is limited to 2 KiB. The same checks run on the
canonical encoder and checked/decode paths before Core conversion; Core then
validates required IDs, enum values, ownership and Port/Profile semantics.
These allocation/semantic bounds do not make a transport authenticated or safe
by themselves. Sidecar control uses the smaller dedicated stdout/stderr limits
and terminal desynchronization policy above.

## Error behavior

An authenticated control channel becomes terminal after any received-frame or
transport I/O failure. Its socket is shut down and later operations return
`ChannelFailed`; partial traffic cannot be resumed or resent on that stream.
Local outbound validation errors occur before I/O and leave a healthy channel
usable. Reconnect and command reconciliation remain host-owned (ADR 1444).

Zero/unknown enums, invalid UUIDs, missing semantic fields, unsupported Profile
majors and incompatible Route endpoints return typed errors. Unknown optional
Protobuf fields remain forward-compatible. Human messages are not parsed.

## Compatibility tests

- Core ↔ Protobuf catalog round trip;
- bounded Session-bound complete-catalog encode/decode round trip;
- Envelope binary round trip and unsupported major;
- Capability/Port/Route/Problem round trips;
- malformed UUID and missing fields;
- unknown/zero enum values;
- size boundaries.

Unknown-field compatibility fixtures and golden wire fixtures remain required
before external protocol consumption.

Diagnostic/UI JSON snapshots are not automatically public protocol schemas.

### Resource consent UI scope (ADR 1498)

Android resource-confirmation JSON adds `capabilityId` and `resourceKind`
(`touchpad`, `gamepad` or `haptics`) to the existing Route/peer/name/expiry fields. These are
native-host-authored scope metadata, not new control Envelope fields. Both must
be present together. The UI still parses the earlier touchpad-only shape;
remembered permissions on the updated host require exact resource identity and
pair generation. Legacy touchpad permission never authorizes a haptics Sink.

### AdapterManaged haptics data offer (ADR 1498)

RouteStatus optional field 4 is `HapticsDataOffer { port, stream_id }`.
It is mutually exclusive with other data offers and only valid for a finite,
authorized, Starting/Active AdapterManaged `capyio.haptics.feedback/1` Route.
The port is 1–65535; no alternate IP or credentials can be supplied by the offer.
An old peer may ignore this appended optional field, but the new initiator must
not claim data readiness without the offer and execution acknowledgement.

The private TLS channel uses ALPN `capyio-haptics/1`, binding magic `CPYHAP01`,
and the existing exact Session/Route/Source/Sink/Stream/epoch/expiry binding.
This framing is not a StandardPort interoperability claim. Each application
record is exactly 24 bytes, big-endian: bytes 0–3 `CPHF`, byte 4 version 1,
byte 5 kind (0 Stop, 1 Rumble, 2 Ack), bytes 6–7 duration milliseconds,
bytes 8–15 sequence, bytes 16–17 low-frequency motor, bytes 18–19 high-frequency
motor, bytes 20–23 reserved zero. Motors use 0–65535; Rumble requires a nonzero
motor and duration 1–160 ms. Stop/Ack require zero duration and motor fields.
Sequence begins with Stop 0, then increases exactly by one; Sink returns Ack
only after command execution. There is one outstanding command, no replay or
resend after timeout, and channel loss cancels the owned actuator. Control
envelopes never carry these continuous feedback records.

### Controller single initiation (ADR 1500)

The one-click controller operation reuses RouteStatus, RouteCommand and Port
leases on the authenticated Session. Android publishes one Gamepad Source whose
private AdapterManaged packet contains controls and optional motion. Windows
publishes distinct DS4, Xbox 360 and motion-only DSU Sinks. Selecting DSU does
not create a virtual controller; it projects motion to the local emulator port.
No independent IMU Route is implied by the combined gamepad input grant.

Either endpoint may propose the Route. A proposal is not permission to start
hardware. The owner of the receiving resource obtains finite resource consent;
the exact source and sink leases must be acquired before startup. A locally
initiated Android proposal is consent only for that exact operation, not future
requests from every paired peer. Pairing alone remains insufficient.

ControllerReceiverOffer is endpoint configuration, not a start command. It is
scoped to the selected authenticated peer and current operation. Android starts
the sender only for the admitted Route with a matching Session offer, valid
authorization and live Source lease. Starting and Active are distinct: the
sender must send input and the receiver must observe live input before the UI
reports usable. Stop and Session invalidation end the owned operation.

The existing token-bearing controller UDP packet remains an adapter-private
transport; this work does not claim a new StandardPort or application-layer
encrypted input protocol. Deployment currently uses the user's Tailscale path.

## Linux local management v1

ADR 1514 defines a local inherited-stdio NDJSON contract, separate from Adapter
JSON-RPC and cross-device Protobuf: exactly `version`, `id` and `command`; version
1 supports `get_snapshot`, `host_status` and `host_shutdown` by default; ADR 1515
below adds opt-in mock commands. Requests are bounded
to 4096 bytes and responses to 65536 bytes, excluding LF. Responses contain `result`
or `error.code`; no media or credential data is carried. See ADR 1514 for rejection
and termination semantics. This is not compatible with the Windows named-pipe API.


## Linux opt-in mock management (ADR 1515)

ADR 1514's version-1 three-field request object and byte limits remain unchanged.
Explicit `--mock-routes` permits parameter-free `mock_route_start`,
`mock_route_stop` and `mock_source_crash`. Otherwise these return
`error.code=mock_mode_disabled`; operation failures use `mock_lifecycle_failed`.
Start/Stop success returns `test_only=true` and current mock Routes. Crash success
returns `test_only=true, source_exit_observed=true`, not a stale Active Route.
The next snapshot/status or repeated Start reconciles the actual child exit.
Lifecycle-only host status includes `mock_routes_enabled` and
`sample_transfer_enabled=false`; ADR 1516 adds a separate experimental opt-in.
These local management commands carry no data payload or production peer grant.
See `adr/1515-linux-mock-route-lifecycle.md` for simulated Session semantics.

ADR 1516 adds separate `--mock-payload` opt-in requiring `--mock-routes`.
Commands remain unchanged. Status includes configured `sample_transfer_enabled`
and nullable `sample_receipt`; Start/Stop also return that receipt. Receipt fields
are Route ID, Stream ID, epoch, received count, payload-byte count and a
`fnv1a64:`-prefixed checksum string, never raw
payload. The bounded private Unix datagram is not this management protocol,
Sidecar JSON-RPC or a public cross-device protocol. Finite IPC acceptance passed
on hosted Ubuntu for commit `4210120`; see ADR 1516 for exact run and scope.


ADR 1517 adds `--mock-batch`, requiring `--mock-routes` and conflicting with
`--mock-payload`. The inherited-stdio management schema and commands are unchanged.
`host_status.mock_batch_enabled` explicitly identifies the selected fixed-batch
mode; `sample_transfer_enabled` is true for either payload mode. Batch receipts
retain Route/Stream/epoch, received count, payload bytes and checksum, and add
`first_sequence=0` and `last_sequence=3`. These two fields are omitted from the
unchanged one-record receipt. Count must be four, total token bytes 84 and the
checksum must match ordered sequence/token consumption before Active. The private
`dev.capyio.test.unix-batch-v1` datagrams remain separate from JSON-RPC payloads
and do not extend the cross-device protocol or claim continuous streaming.

ADR 1518 reuses the existing native microphone lab UDP framing unchanged for
opt-in Linux PCM playback. It adds no Envelope, control or data wire. Fixed a200
lab identities are not the Android integrated dynamic Route workflow. Local CLI
stdin is Stop/EOF lifecycle only, never media; evidence is forwarded PCM bytes,
not a Runtime Active transition or authenticated peer.

ADR 1519 adds no control or wire messages. A Linux library worker consumes the
existing dynamic AudioMediaStreamBinding and live Runtime/lease state, without
accepting those assertions from serialized peer input. The unchanged static CLI
and inherited-stdio host cannot start this worker. Authenticated Linux control
composition and secure media remain separate gates.

ADR 1520 reuses unchanged authenticated control RouteStatus/native-audio offers,
heartbeat and exact-Route Stop commands for a dedicated Linux playback Session.
Unknown message families fail closed in this bounded owner. No new wire fields
or peer-provided Ready/authorization injection API is introduced. Stop closes
the dedicated channel; it does not promise a graceful wire acknowledgement.

The opt-in deadline APIs bound whole control-frame operations, not each received
fragment. Expiry closes that connection; partial frames cannot be retried. No
wire representation changes, and legacy callers retain their existing API.

ADR 1521 changes no wire format. Opt-in incremental TLS/link polling retains
bounded prefix/body state, returns at most one authenticated message per step,
and starts a nonrenewable application-frame deadline at its first plaintext
byte. Idle returns no message. Existing blocking receive behavior is unchanged
unless mixed with a partial polled frame, which fails closed.

ADR 1522's separate opt-in launcher protocol uses bounded u32 big-endian framed
stdin/stdout, not the inherited-stdio management protocol or network Envelopes.
Three one-shot bootstrap frames precede authenticated catalog exchange and an
exact nonce/Route local approval challenge. Stop/status are separate local
commands. Full field limits/order and fixed error events are specified in ADR1522.
Existing network protocol and Android/Windows messages are unchanged.

## Explicit incoming native audio Draft contract (ADR 1529)

The existing bounded Capability metadata map may advertise
`dev.capyio.native-audio.inbound-draft = pcm-s16le-48000-mono-v1` only when its
Adapter implements the incoming finite-consent mono PCM handshake in
[ADR 1529](adr/1529-linux-incoming-audio-draft.md). This is not permission or a
claim that arbitrary audio Profile ports interoperate. Android uses the actual
compatible advertised Sink IDs, and unknown values remain unsupported. Linux's
default Linux-led mode omits the key. Exact Pending Draft admission precedes
independent local approval; finite Linux grant then bounded Android grant precede
all leases and media. No protobuf field, authentication policy or UDP protection
is added.

## Finite secure camera transport (ADR 1534)

`RouteStatus.secure_camera_data` is a separate additive field 6. Its offer carries
nonzero actual port, Stream UUID and explicit TLS13 mTLS v1 transport; no host
field is accepted. The destination IP comes from the authenticated control
channel. Reject unknown versions and multiple data offers. Legacy field 5 remains
plain controlled-lab CAVC on fixed port 38173, never a fallback for secure data.
Camera TLS uses ALPN `capyio-camera/1`, binding magic `CPYCAM01`, and the existing
208-byte complete Route binding/ACK. Each complete CAVC record is prefixed by a
u32 big-endian length bounded to 4 MiB + 56 bytes before allocation. Existing
profile u16/176-byte transport records remain unchanged. The one-shot camera
listener closes after successful binding ACK. See ADR 1534 for finite authority,
Source/Sink leases, cancellation and explicit Android synthetic-proof scope.

### Explicit finite camera stream profile

[ADR 1555](adr/1555-linux-explicit-finite-camera-stream.md) defines the additive
secure-only `cavc-avc-finite-720p30-v1` descriptor and local immutable work/rate
budgets. Legacy wire formats and limits remain unchanged. ADR 1557 adds a separately gated, explicit experimental Android capture option;
legacy remains the default.

### Android Rust finite camera sender

[ADR 1556](adr/1556-android-rust-finite-camera-sender.md) adds independent finite
profile admission to the actual Rust sender and immutable JNI generation binding.
ADR 1557 subsequently adds explicit experimental eligibility and actual Java
output admission; ADR 1558 adds bounded cardinal pixel normalization. Synthetic sender-to-Linux evidence still does not
establish phone compatibility.

### Experimental Android finite capture

[ADR 1557](adr/1557-android-experimental-finite-capture.md) adds explicit opt-in
eligibility and actual output checks. [ADR 1558](adr/1558-android-finite-camera-pixel-orientation.md)
adds a finite-only GPU pixel stage for static cardinal sensor orientations on
Android 13+, preserving canonical 1280×720 rotation-zero output. Quarter turns
fit the full portrait image into the landscape canvas with black sidebars; no
front mirroring or live display-rotation tracking is implied. Eligibility and
hosted synthetic GPU evidence are not physical-device compatibility. Legacy
remains default; settings changes cannot extend a grant or start capture.
