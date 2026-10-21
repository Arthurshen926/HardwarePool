# ADR 1550: Interactive finite paired-store virtual camera

Status: experimental; isolated terminal and independent pixel acceptance required.
Extends ADRs 1547–1549 without changing the Node camera protocol.

## Scope

The console adds `camera --config ABSOLUTE_PATH --peer NODE_UUID`. This is a
finite remote camera Source → local virtual-camera Sink path, not local physical
camera capture or a device picker. Touchpad console control, persistent resource
grants, automatic renewal/reconnect and unattended services remain out of scope.
Pairing-management commands and all existing audio paths retain their boundaries.

Only the trusted fixed sibling `capyio-node linux-camera --trusted-lab` is used.
The same four explicit paired-store selectors are required, with no default bus,
unlock, keyring UI or protected credential export. The nonsecret public manifest
and distinct paired-store marker travel only over inherited anonymous pipes.
The console probes actual sibling capability before opening its controlling TTY.
Running the command permits the finite authenticated listener, not a resource
permission. Native camera support and existing isolated helper prerequisites must
be present in the chosen build; help alone does not establish physical readiness.

## Strict scope and fresh consent

The manifest contains only version, local node, exact remote Source Port, concrete
supported local IPv4 listen address and a 3–60-second duration. It uses the same
bounded owned regular-file rules as the audio console. Reading a filesystem is
not a hard wall-clock bound; it occurs before starting a listener or resource.
Unknown/audio fields, implicit peers, invalid identifiers and unsupported values
fail closed.

Only the current authenticated Node approval event opens the terminal prompt.
The controller validates exact remote Source, local Sink node, nonnil request,
route and session IDs, `cavc-avc-access-unit-v1`, `tls13_mtls_v1`, finite duration,
30-second consent window and the existing fixed 1280×720, 30 fps, zero-rotation,
120-frame and 33554432-byte limits. It displays peer, direction, virtual resource,
format/security and all finite bounds. The user must type the exact fresh
`start REQUEST ROUTE PEER` after prompt flushing. Pretyped, stale or mismatched
input cannot approve a later event. The Node's own current request check and
independent remote Source grant/lease remain necessary.

Shared framed-event decoding now rejects duplicate JSON keys recursively before
creating a generic value; direction-specific typed validation still rejects
unknown fields and malformed shapes. Existing frame size/depth bounds remain.
This closes an ambiguity at the shared console boundary, not a new wire protocol.

## State and cleanup

Canonical public events use `Camera event:`. Before binding, status requires no
helper/decoder/node and a zero queue. First binding latches the exact helper and
decoder IDs plus canonical nonnil stream-derived `capyio.camera.UUID` name;
subsequent records/status cannot change them. Queue counts are nondecreasing and
bounded by the exact frame limit. All running camera events must remain
`Starting` and `consumer_delivery_verified=false`. Local approval, a helper,
binding or queued frame never becomes an `Active`/consumer-delivery claim.

The existing authenticated control and camera media transport retain their
pinned mutual-TLS semantics, unlike the separately documented plaintext audio UDP
path. This distinction does not prove consumer use or physical-device operation.
`status`, `stop`, terminal EOF/interruption, peer EOF, revocation and finite expiry
are one-shot controls. The controller adds finite waiting/consent/Stop bounds but
never extends the Node grant or lease. Stop closes/reaps only its owned child,
uses the existing Node cleanup path and retains pairing credentials. No retry,
renewal, permission inference from pairing or automatic listener restart exists.

## Acceptance boundary

Thirteen mandatory hosted profiles drive an actual console through an owned PTY,
the fixed sibling Node, private fake Secret Service and pinned synthetic peer.
The existing private PipeWire camera fixture independently checks pixel content,
actual Node/decoder/helper ancestry, graph ownership and cleanup. The profiles
cover pixels, no approval, pretyped input, wrong request/route/peer, expiry,
remote EOF, local Stop, remote Stop, Source revocation, guarded sender rejection
of a partial camera record and handshake timeout. The partial-record case does
not claim those malformed bytes reached the decoder. Pending/handshake cases must have no camera graph; they cannot
substitute for independent pixel proof. Exact console markers cannot stand in
for an executed test or a legacy Node-only run. Expiry observations check the
actual negotiated Source lease, not the longer requested grant. Bounded polling
checks terminal events and Node/console exit without an early-expiry tolerance;
it does not claim an exact internal failure timestamp between polls.

All prior 64 audio graph profiles and legacy/paired camera profiles remain
mandatory. Existing native helper safety bounds and failure tests are retained.
Tests use synthetic identities, private buses, owned PTYs and software graphs,
not a user's keyring, terminal, physical camera or desktop session. No production
cryptography, Node authority, lease semantics or native dependency is changed.
