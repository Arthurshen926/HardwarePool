# CapyIO Security Model

> Status: pre-alpha threat model. Production pairing/cryptography are absent.

## Protected assets

- live audio/video, input events, location and sensor data;
- actuator/haptics/control commands;
- node identity, trust records, pairing/session secrets;
- Capability/Route authorization and system Projection availability;
- Adapter executable integrity and child-process control;
- host stability, especially OS services and kernel components;
- recordings, diagnostic logs and test artifacts.

## Actors and trust boundaries

Actors include local user, paired peer, unpaired peer, compromised paired peer,
malicious local process, administrator, faulty/malicious Adapter, UI/WebView and
supply-chain attacker. Pairing does not authorize every Capability.

```text
untrusted network -> authenticated control/data -> Node Runtime
Node Runtime -> Adapter Host -> Sidecar/external service -> platform/driver
UI WebView -> allow-listed local API -> Node Runtime
```

## Principal threats and controls

### Unauthorized sensors/actuators

Capability/Route-scoped requests, provider confirmation, expiring leases,
immediate revoke, visible platform lifecycle and persistent use indicators.
Microphone/camera/screen capture never starts silently.

### Eavesdropping, injection and replay

Production paths require mutual authentication, authenticated encryption,
Session/Route/epoch binding, replay windows, fresh keys and version binding.
The current Mock/Sidecar loop is local test behavior, not a secure network.
The Gate 5 SensorServer `ws://` protocol Adapter is likewise restricted to an
explicitly approved trusted lab. A Tailscale address can protect overlay transit
but does not supply CapyIO Capability authorization, application identity,
message replay protection or downgrade binding.

The SensorServer lab client rejects DNS names, arbitrary paths, credentials,
redirects, binary sensor payloads and frames/messages above 4 KiB. TCP and
WebSocket operations have deadlines. These controls limit attack surface but do
not authenticate the external app or authorize a Capability.

The Audio Share lab likewise requires an explicit IP-literal bind address,
non-zero port and enumerated playback endpoint; the CLI is launched directly,
never through a shell. Probe output, lines and deadlines are bounded. These
controls do not authenticate the Android receiver or secure Audio Share's
private TCP/UDP protocol. The pinned upstream Windows release is not
Authenticode signed and is neither bundled nor treated as trusted production
software by CapyIO.

The dedicated virtual-speaker Broker is a separate trusted-host mode. It
requires a non-unspecified IPv4 bind address and accepts no playback endpoint
identifier; it can consume only the fixed, versioned CapyIO render ring. Its
child process is launched directly with bounded output and is reaped on an
explicit stop or host shutdown. This lifecycle control does not add peer
authentication or make the private Android transport production-safe.

ADR 0033 moves that Broker into an SCM-managed Windows service so the WebView
host need not inherit the privilege required for the cross-session global
mapping. Service launch configuration remains administrator-controlled and
accepts only a direct executable path, explicit IPv4 literal and bounded port.
ADR 0034 adds one local-only named pipe for bounded `status`, `start` and `stop`
operations. The protected DACL grants LocalSystem/Administrators full access
and interactive local users explicit read/write access; remote pipe clients are
rejected. Closed schema-v1 frames are limited to 4 KiB and have fixed I/O
deadlines. No request can change executable paths, bind addresses, ports or
endpoint identity. Any interactive user can currently control the machine-wide
Route, so multi-user authorization remains unresolved release work.

The desktop Quick Action never accepts an executable path, endpoint identifier,
bind address or port from the WebView. Those values come only from the trusted
host environment, while the versioned UI request is closed to unknown fields
and permits only one stable action ID plus `start`, `retry` or `stop`.
Start-time endpoint re-probing reports disappearance through a stable sanitized
Problem and does not echo the configured endpoint ID into UI diagnostics.
Endpoint reselection exposes only bounded display names and short-lived opaque
tokens. The Tauri host accepts a bounded token only from its latest enumerated
allow-list, rejects unknown/stale tokens and active-Route changes, and never
accepts a raw endpoint ID from the WebView. Scan failures return a stable
sanitized message rather than upstream stderr. The selection is not persisted.

Windows receiver observation filters the OS TCP owner table by the supervised
process ID, explicit local port and established state. It returns only a count,
not local/remote addresses. An unauthenticated or unrelated TCP peer can still
produce presence in the current lab, so this signal is not peer identity,
authorization, successful negotiation or proof of audible playback.

### Adapter process abuse

Versioned manifests, allow-listed entrypoints, bounded NDJSON, correlation
limits, stderr-only logging, timeouts, least privilege and ownership-scoped
failure. A manifest is not a sandbox; production distribution additionally
needs signing and provenance.

### UI command abuse

Narrow DTO commands, Rust-side validation, CSP, no remote WebView content and no
arbitrary shell/filesystem/updater plugin in the foundation.

The synthetic-touchpad injection harness is a local controlled-lab binary, not
an allow-listed WebView/Node Runtime command. Its default path only projects and
encodes fixed fixtures. Native submission requires both an explicit injection
mode and a separate acknowledgement, accepts no arbitrary coordinates or
repeat count, and reports whether any batch was submitted. These CLI gates are
an accidental-use safeguard, not production authorization or peer trust.

The reusable synthetic-touchpad Sink session deliberately has no CLI gate. It
is a low-level platform capability that must only be constructed after the
Runtime has validated the authorization tuple. A native submission failure
poisons the session, bounded cancellation is attempted, and device destruction
is the final cleanup. The Sink does not accept network messages, perform
pairing/reconnect, or authorize a peer.

The private remote-touchpad packet decoder is also not a trust boundary. A
future transport must authenticate and authorize the peer/Route, bind the
negotiated Stream and descriptor, and reject unauthenticated traffic before
constructing a receiver. Structural decoding is fixed to at most 152 bytes and
five contacts, but valid structure is not permission to inject OS input.

The Adapter receiver adds defense-in-depth after that authorization boundary:
strict per-epoch sequence acceptance, a 1..=1000 packets/s fixed-window bound,
trusted-local arrival-clock checks and a 10 ms..=30 s active-contact idle
deadline. Decode, replay, rate, clock or Sink failure poisons the receiver and
attempts cancellation/close. This is not cryptographic replay defense or
network admission control; a future authenticated transport still owns those
properties and must schedule the timeout poll even when no packet arrives.

The private Route ingress consumes a current Runtime-owned `Route` snapshot at
construction, activation, enqueue and pump. It checks the exact Route/Session
and endpoints, expected Sink Port, touchpad Profile, AdapterManaged backend,
authorization state/expiry and epoch. A typed or structurally valid Route value
is not a credential: the Adapter host must prevent an untrusted transport or UI
from fabricating this argument. The fixed queue holds at most 64 records and
fails closed on overflow, stale queue residence, clock regression or Route
invalidation. This narrows time-of-check drift inside one process; it still does
not supply peer identity, authenticated grant issuance or encrypted transport.

The private worker obtains Route/time inputs from composition-owned traits
instead of trusting packet-callback arguments. One immutable Route snapshot is
used for each action, and rollback in either authorization milliseconds or
ingress nanoseconds fails closed. Route-provider or clock failure also closes
the Sink. This reduces confused-deputy and stale-snapshot risk inside one
process, but the provider itself must remain inaccessible to untrusted peers and
does not replace authenticated transport admission.

The Android capture boundary retains at most five active pointer IDs in fixed
storage and accepts motion only while explicitly running. Cross-event pointer
identity drift, invalid action transitions and mapping/timestamp errors fail
transactionally. Start, lifecycle stop and close produce semantic cancellation
before state changes. This is not an Android permission, JNI or peer-trust
boundary and opens no network or platform component.

Its multi-finger attenuation also uses fixed five-contact anchor storage and
bounded integer arithmetic. It neither infers nor emits OS gesture commands,
and it cannot broaden Route or injection authority. Added-pointer MOVE settling
occurs in the explicit foreground lab Activity; down/up/cancel and pointer-count
transitions are never discarded.

An Android `ACTION_CANCEL` after three or more observed contacts is diagnostic,
not permission to recreate contacts or bypass an OEM monitor. The debug Activity
may offer an explicit user-driven intent to a vendor settings page, but it does
not write hidden settings, disable packages or request privileged permissions.
If the vendor settings Activity is unavailable, only generic Android Settings
is opened.

The private packet Source independently requires the cancellation barrier and
contiguous sequence before producing bytes. Rejected frames do not commit its
tracker or metrics, active contacts prevent close, and closed Sources reject
further input. It has no transport authority: structurally valid packet bytes
are not peer authentication, authorization or permission to inject input.

The delivery handoff requires a trusted-host channel to reassert the complete
Route binding on every operation. Admission loss, endpoint/Session/epoch/lease
drift and uncertain delivery close the channel and poison the session before
further input. Only a channel guarantee that no write occurred permits the same
frame to be retried. This contains replay ambiguity but does not implement or
claim mutual authentication, encryption, key freshness or network replay
protection; the current admitted channel is a trait with fake test providers.

The sender Runtime worker does not accept Route snapshots or timestamps from a
packet producer. It reads one current Runtime-owned Route and one coherent local
clock sample for construction, every frame and normal close, derives the exact
Active Source binding, and requires that tuple to remain unchanged. Provider or
clock failure, rollback, expiry and Route drift close the channel before more
input is accepted. This reduces stale-snapshot and confused-deputy risk inside
the trusted host; it does not authenticate the channel implementation or peer,
protect packet bytes, issue grants or replace transport replay protection.

The concrete host channel is an in-process capability handed only to the
trusted composition layer. Its admission controller can deny or replace the
complete Route binding; either action clears pending packets before a later
lifecycle can receive them. Receiver close also clears the queue and makes
subsequent sends definite pre-write rejections. `Rc<RefCell<...>>` deliberately
keeps the channel single-threaded and non-networked. Possession of this local
handle is not peer authentication, and the type must not be exposed directly
to untrusted UI, Android or network input.

The 002S native acceptance is compiled from one closed fixed fixture and is
excluded from default CI. Its exact invocation requires explicit human
authorization because it creates a real user-mode synthetic touchpad and can
move the desktop pointer. It installs no driver and changes no boot/security
policy. The test submits release before bounded Sink close and Runtime stop;
RAII cleanup remains the fallback if an assertion fails.

ADR 0045's Hello is protected binding confirmation, not peer authentication.
The surrounding connection must already authenticate and encrypt both peers
before any record is accepted. Every Hello byte is compared with the current
Runtime Route binding; Data repeats epoch/sequence outside and inside the packet.
Unknown or mismatched records fail before delivery. Ack loss is treated as
delivery-unknown and forces cancellation/new epoch, preventing optimistic retry
from duplicating desktop input. The current codec retains no keys or replay
window and therefore cannot be used directly on an untrusted network.

ADR 0047 permits only the first explicitly authorized physical touchpad lab to
place those records inside an ADB-paired reverse tunnel. The Windows endpoint is
fixed to loopback, the Android endpoint is device loopback, and the receiver
still requires full Hello equality plus two explicit desktop-input flags. The
debug APK declares only the approved Internet permission and is hash-checked
after installation. ADB pairing is not CapyIO production identity; direct
LAN/WAN use, production authorization, key rotation and reconnect remain
forbidden until a later transport ADR. Hosted CI never installs the APK or
creates the reverse mapping/device.

Synthetic Sink creation uses a validate-before-open factory boundary. Route
authorization/expiry, exact endpoint binding, epoch, Stream/descriptor and all
queue/receiver bounds are checked without a platform Sink. Only a successful
preflight may invoke the Windows factory. Tests prove invalid Route and semantic
contract inputs leave the factory open count at zero. This narrows construction
side effects but does not authorize a peer or approve a physical device test.

The controlled real-factory acceptance is excluded from default CI and requires
an exact ignored-test invocation after explicit human authorization. It keeps
the Route in Starting state, confirms zero queued/processed packets, closes the
Sink immediately and completes Runtime stop. No contact frame or gesture is
submitted. RAII drop and process exit remain fallback device-destruction paths.

The separately authorized one-finger acceptance is also exact-name and ignored.
It submits only four compiled semantic frames, uses one packet/pump at a time,
ends with an empty release snapshot, verifies exact processed counts and then
closes the Sink and Runtime Route. It accepts no file, network or user-provided
payload and grants no reusable desktop-input permission.

The separately authorized two-finger acceptance uses two fixed contact IDs and
four compiled frames only. It can scroll the active foreground surface, is
ignored by default, ends with an empty release and follows the same bounded
close/Drop recovery path. No arbitrary gesture or payload interface is exposed.

The separately authorized three-finger acceptance is also exact-name and
ignored. It submits only the compiled 11-frame horizontal-swipe fixture at a
fixed 15 ms interval, may switch the active Windows desktop or application,
ends with an empty release plus CancelAll, and then closes the Sink and Runtime
Route. The authorization does not extend to four-finger or arbitrary input.

The separately authorized four-finger acceptance has an independent exact-name
ignored test and approval. It submits the compiled 11-frame fixture only, may
trigger the configured Windows system action, and uses the same release,
CancelAll, close and Drop recovery chain. It creates no reusable arbitrary-input
surface and changes no driver, boot or security policy.

### Kernel compromise

No network, DNS, pairing, crypto negotiation, JSON/Protobuf, codecs or reconnect
logic in drivers. Kernel IPC is fixed/bounded and validated. Driver tests
default to an isolated approved target. ADR 0029 permits bounded Gate 7B
install/enumeration/playback/uninstall checks on the identified local lab after
recovery and rollback preflight. Driver Verifier and boot/security-policy
changes remain separately approved high-risk actions.

The VHF Precision Touchpad fallback exposes one buffered IOCTL through a device
interface whose driver SDDL and INF registry security both grant access only to
LocalSystem and Built-in Administrators. Both the function object and installed
stack are exclusive. Records are exactly 50 bytes with closed version/kind,
payload length, contiguous sequence, canonical zero padding and no more than
five validated active contacts. A VHF submission failure poisons the file
session. This limits local kernel input attack surface but does not authenticate
a remote peer; the future privileged Broker must retain Runtime Route admission
and must never forward untrusted transport bytes directly to this ABI.

The user-mode VHF transport enumerates only the compiled device-interface GUID,
requires exactly one present interface, caps SetupAPI detail storage, opens
without sharing and accepts no path from UI or network input. It performs one
synchronous fixed-size IOCTL at a time and requires an exact canonical Ack. A
Win32 failure, short output or Ack mismatch poisons the client because delivery
may be unknown. This client still has no authority to create a Route or admit a
remote peer; composition with the existing Runtime-owned authorization tuple
remains mandatory before any physical-input deployment.

The VHF Sink session composes validated semantic frames with exactly one
Broker client and exposes no network, UI or arbitrary device-path input. It
validates the descriptor before transport open, makes unknown delivery
terminal, and uses the driver's Close operation for bounded release on explicit
close or active Drop. Its `open_win32` helper is an authority-bearing operation,
not authorization: only the trusted Adapter factory may call it after current
Runtime Route admission has been revalidated.

The VHF Adapter factory reuses the shared validate-before-open worker boundary.
Exact current Route binding, authorization expiry, Stream epoch, descriptor and
resource limits are checked before the factory can enumerate or open the
protected interface. Epoch advance closes the previous Broker generation before
rebinding the new one; either acknowledgement ambiguity faults the Sink. The
factory is still a privileged local capability and must not be exposed to the
WebView or remote packet producer.

The 003F package builder pins source hashes, refuses overwrite, uses a temporary
non-exportable CurrentUser signing key and deletes that private key after
exporting the public certificate. It never imports the certificate into Root or
TrustedPublisher. The rollback script matches only the exact CapyIO provider,
INF and root hardware-ID prefix and refuses ambiguity. Self-signing does not
make the package trusted or deployment-safe; recovery, trust, test-signing,
Secure Boot and reboot decisions remain separate controls.

The 003G failure demonstrates why deployment approval and rollback remain bound
to exact artifacts: a correctly signed package can still fail device start.
The 003H installer additionally requires PnP status `OK`, refuses any restart
request, and invokes a rollback pinned to version 0.0.2.0 and the 003H
certificate thumbprint. Attaching Microsoft's `vhf` lower filter does not move
network, pairing or untrusted packet parsing into the kernel boundary.

The `003J..003N` VHF desktop acceptances are ignored by default, require an
elevated administrator process and pin one exact test executable hash. Each
exact test submits only its compiled fixed fixture, releases contacts and closes
the Broker generation. The Android loopback lab receiver may select the same
installed VHF Sink only with the existing `--inject` and
`--acknowledge-desktop-input` gates plus an explicit `--vhf` option. It validates
the full Hello binding before opening the protected interface. This remains a
privileged, ADB-paired local-lab path rather than a production trust boundary;
it cannot listen beyond `127.0.0.1` and grants no WebView or remote caller direct
access to the Broker device.

ADR 0049 centralizes the post-authentication transport-to-Sink ordering in a
pure Adapter state machine. Receiver bounds and Route/Stream epoch equality are
checked before the factory is retained; a mismatched full-binding Hello leaves
the platform open count at zero. Ack is returned only after Data passes both
outer/embedded identity checks and Sink submission. Malformed transport closes
the active Sink. The object deliberately accepts a caller-supplied binding and
therefore is not Route authorization, local-client identity or remote-peer
authentication; a future service must supply all three before exposing I/O.

The first `CapyIO Speaker` path uses the standard Windows render/loopback model
so PCM remains observable in user mode without a new custom render-ring IPC.
Any later custom driver IPC must be justified by retained measurements and must
retain the same fixed-size, versioned and fail-silent boundary.

### Denial of service and stale state

Bound message/queue/event/log sizes, rate/deadline limits, disconnect cleanup,
fresh epochs, bounded child shutdown and safe OS endpoint behavior when user-mode
processes disappear.

### Supply chain

Pinned toolchains/lockfiles, dependency review, provenance manifests, retained
licenses/notices, SBOM/signing before release and no secrets in untrusted builds.

## Authorization tuple

```text
requesting_node
providing_node
capability_id / port_id
route_backend and constraints
session_id
issued_at / expires_at
```

Opposite-direction or compound-device Routes have independent grants and may
partially fail.

## Logging/privacy

Default logs may include typed IDs, states, timings, counters and sanitized
errors. They exclude raw content, pairing codes after use, private/session keys,
tokens, unrelated process lists and personal filenames. Recordings are explicit
artifacts with retention/deletion policy.

## Milestones

1. Foundation: deterministic validation and process boundaries.
2. Local real-Adapter lab: clearly marked insecure/local mode.
3. Pairing spike: device identity and authenticated transcript.
4. Production transport: encryption, replay and downgrade binding.
5. Projection/driver hardening: ACLs, isolated tests, fuzzing and review.
6. Release: signing, SBOM, vulnerability and update process.

No build before milestone 4 is described as safe for untrusted networks.
