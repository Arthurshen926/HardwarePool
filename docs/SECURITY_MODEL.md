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

The integrated Runtime now provides a post-authentication peer-catalog
admission boundary. It accepts a complete catalog only for the exact remote
Node bound to a Ready Session, preserves transport-owned online state and
atomically reconciles dependent Routes. Wrong identities and suspended
Sessions leave Runtime state unchanged. This prevents cross-Session catalog
confusion inside the trusted host, but does not authenticate an envelope,
establish a transport, issue a Capability grant or make the current lab wires
safe on an untrusted network.

`capyio-control-host` composes those two boundaries for one caller-asserted
Session/peer binding. It rejects mismatched Session and Node identities before
Runtime mutation and remembers at most 256 successfully applied message IDs so
an immediate duplicate is not applied twice. Constructing the binding is not
authentication, and bounded recent-ID suppression is not cryptographic replay
defense: an authenticated transport must bind identities and versions, reject
replays across reconnect/process restart and supply fresh keys before invoking
this host.

ADR 0041 now represents Session/Route/Stream/epoch and the exact selected audio
specification as one validated media-stream binding before any transport
backend. This prevents platform and codec code from inventing a second identity
model, but the in-process binding and bounded packet queue provide no peer
authentication, encryption or replay defense by themselves. A network backend
must cryptographically bind that control-approved value before production use.

ADR 0042 makes backend security claims explicit alongside metadata fidelity.
The current Audio Share and MicYou compatibility contracts set peer
authentication, confidentiality, integrity, replay protection and downgrade
binding to false. The descriptor prevents UI/runtime code from mistaking a
private compatibility path for a production-secure backend, but does not add
security to either wire.

ADR 0044's native LAN reference also sets every production-security property
to false. It accepts one configured unicast IP/port, rejects a different UDP
source and validates Session/Route/Stream/epoch plus canonical fragmentation,
but source-address filtering and self-asserted header IDs are not
authentication. The wire has no encryption, integrity tag, replay window,
downgrade binding, pairing, Capability grant or key lifecycle. It is allowed
only on an explicitly trusted local/Tailscale lab; Tailscale transport does not
replace CapyIO application authorization. Android cleartext app traffic remains
disabled, but that setting is not cryptographic protection for raw UDP.

ADR 0046's speaker build metadata is closed and outside Activity input, but it
is still only trusted-lab configuration. A matching source IP, port and
self-declared Route tuple do not prove peer identity. The default Android build
disables the lab receiver unless an explicit peer IPv4 is supplied at build
time.

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

The VIIPER Xbox 360 boundary accepts only an explicit IP-literal loopback socket
with a non-zero port, positive bounded connect/I/O deadlines and a response
limit no greater than 4 KiB. It has no default address, DNS, discovery,
authentication-key lookup or generic request API. Independent read-only probe
sends exact `ping\0`, reads JSON through connection close under one absolute
I/O deadline, uses fixed DTOs and accepts only exact server/version identity.

ADR 1200 governs resource creation. The mutating entry point requires a caller
assertion that the separately supplied server has localhost auto-attach
disabled, then re-probes compatibility and owns create/add/stream/initial
neutral/remove as one operation. Only the returned positive bus ID is eligible
for cleanup; arbitrary CRUD and enumeration are not exposed. Once known, that
bus is removed after open failure or explicit stop, and cleanup failure cannot
replace the primary error. Drop performs socket shutdown only and is not the
cleanup guarantee. The assertion token records policy but cannot prove an
external process flag; a live Adapter must retain independent configuration
evidence. A malformed create response can leave no safely identifiable cleanup
target and remains a declared external-protocol risk.

Upstream localhost device creation may trigger automatic USB/IP attachment,
and the reviewed v0.7.0 source can return an add error after creating a device
without rollback. Older test-signed usbip-win2 candidates could change the
trusted-root/boot posture and remain prohibited. The selected v0.9.7.7 x64
package is Authenticode/attestation signed; its exact digest, signature chain,
driver package, restore point and rollback still require separate approval and
evidence. VIIPER authentication would not replace CapyIO pairing, Capability
authorization or replay protection.

The Windows input composition layer does not launch or reconfigure VIIPER. It
requires the ADR 1200 caller assertion before installing a Route controller,
uses the Runtime epoch as the fixed gamepad stream epoch and marks the Route
Active only after the owned session is fully open. Open rollback, upstream
disconnect, terminal stream failure, sequence exhaustion and explicit stop
all attempt Worker cleanup before the corresponding Runtime transition.
Problems reference only the gamepad Route and VIIPER Adapter; no failure path
mutates an independent IMU Route. Raw two-byte feedback polling is liveness
evidence only and does not authorize or construct a reverse haptics Route.

The optional Windows USB/IP owner accepts only an absolute executable named
`usbip.exe`, exact version `0.9.7.7`, an explicit IPv4-loopback server and a
VIIPER-derived simple bus/device identity. It lists before mutation and requires
Xbox 360 VID:PID `045e:028e`. Child processes are launched directly with no
shell, no stdin and (on Windows) no console window; stdout/stderr are drained
concurrently under fixed byte limits and each command has an absolute deadline.
Attachment always includes `--once`, retains the returned non-zero hub port and
never uses all-device detach. Explicit Route cleanup writes neutral, detaches
that exact port and only then removes the VIIPER bus. Driver deployment,
restart, removal and boot/security-policy changes are not reachable through
this API and remain separately approved operator actions. The mutating call
also requires a caller assertion that package/signature checks, driver health,
any required restart and attachment authorization are complete; like ADR 1200,
the token records policy but cannot prove external state.

The returned port is not trusted merely because `attach --terse` printed a
number. Before ownership is published, and throughout the bounded physical
lab hold, `usbip port <owned-port>` must show exactly one in-use port with the
fixed loopback server, the expected VIIPER bus ID and Xbox `045e:028e`
identity. Missing or mismatched status fails closed and attempts detach of only
that returned port; status output is bounded and never authorizes a different
port or all-device cleanup.

The DSU composition path binds only IPv4 loopback and activates its Runtime
Route only after the exact port and fixed-epoch stream anchor are valid. Bind
failure is projection-owned; source epoch mismatch or SensorServer disconnect
is source-owned. Both release the Worker before Offline/Stopped, and Problems
remain related only to the DSU Route. The `capyio-gamepad-dsu-lab` command
accepts an explicit phone IP and port but does not print the IP. SensorServer's
plain `ws://` connection has no CapyIO authentication, confidentiality or
pairing and is restricted to an operator-controlled physical lab; DSU client
subscription proves local delivery only, not trusted input or game
compatibility.

The desktop Controller diagnostic likewise accepts only semantic button/axis
updates and one non-zero DSU port from the WebView. The Rust host owns stream
identity, epoch, sequence, bounds and complete-state composition; the DSU
socket is fixed to IPv4 loopback. Browser Mock opens no socket. Other local
processes can still subscribe to or send datagrams to this unauthenticated lab
endpoint, so it is a local Projection fixture rather than a trusted peer or
production input path.

The Windows gamepad preflight command accepts no WebView parameters. Trusted
host configuration fixes VIIPER to `127.0.0.1:3242`, usbip-win2 to
`C:\Program Files\USBip\usbip.exe` v0.9.7.7 and its server to
`127.0.0.1:3241`. Only exact Xbox 360 match count and bus identity reach the
DTO; unrelated USB exports and raw process errors are omitted. Multiple exact
matches fail closed. Concurrent preflight invocation is rejected by a host
single-flight guard. This command has no create, attach, detach, driver,
restart or boot-policy operation.

The Android Controller Lab expands the desktop listener to an operator-selected
IPv4 UDP port on trusted LAN interfaces. Its random 48-bit hexadecimal token is
shown locally and required in every closed-schema frame; comparison is bounded,
the token and peer address are not logged, frames are capped at 2 KiB, and
session sequence rollback is rejected. This filters accidental/unrelated LAN
traffic but is not identity, encryption or production pairing: possession of
the token permits input injection. The listener requests neutral after 350 ms
without a valid frame and on stop. The authorized APK declares only
`android.permission.INTERNET`, keeps capture foreground-only and has backup/data
transfer disabled so the lab configuration is not exported.

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

ADR 0040 does not put MicYou in that LocalSystem service. The privileged
service owns the fixed global capture mapping, while an ordinary-user headless
host reads the fixed user-local MicYou configuration and launches the external
process. Its separate local-only pipe uses the same 4 KiB bounded framing but a
protected DACL granting the object owner, LocalSystem and Administrators rather
than all interactive users. Requests remain closed to `status`, `start` and
`stop`; responses expose only typed state, generation, validated bind address
and stable problem codes. They never carry an executable path, endpoint
identity or arbitrary argument. This isolates local-user process authority but
does not authenticate the Android peer or encrypt MicYou traffic.

ADR 0047 adds a separate native microphone mode to the same privileged Broker.
Its executable and literal local/peer UDP endpoints remain administrator-set
service configuration and cannot be supplied through the control pipe or
WebView. The child accepts only one fixed common-packet binding and explicit
peer, but ADR 0044 still provides no authentication, confidentiality, integrity,
replay protection or downgrade binding. Explicit peer filtering is not peer
identity. The capture ring is SPSC, so the native receiver and MicYou ingress
are mutually exclusive producers. The service does not launch both; manually
starting a compatibility producer beside native mode remains unsupported.

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

ANDROID-A01C2/A01E load only the build-packaged shared presentation from the
fixed `https://appassets.androidplatform.net/capyio-ui/` origin. The custom
asset client validates the exact scheme, host, absent port and bounded path,
serves only APK assets and returns 403 for all other requests. File and content
access remain disabled. Allowing the WebView network stack is required for the
intercepted HTTPS origin; it does not create an external fetch path because the
client fails closed. The bridge is removed when the Activity is destroyed.

ANDROID-A01D1 and later slices extend that bridge only for closed local
Capability actions. The presentation may read bounded Quick Action DTOs and submit one exact
action ID plus `start`, `retry` or `stop`. A dependency-free parser checks the
ID, operation length, current lifecycle state and permitted transition before a
Binder request is queued. Unknown action/haptics IDs, unknown operations,
shell-like strings and invalid transitions are rejected. The command cannot
carry an endpoint, Route, payload, permission, file path or platform handle and
does not grant sharing or peer access.

ANDROID-A01D2 adds a separate closed gamepad-state method. Its fixed `kind` and
`target` vocabulary, integer bounds, unused-field checks and eleven-button mask
reject malformed or extended commands before state construction. Accepted input
is merged into a complete snapshot and submitted with the current generation;
the Service rejects input unless that generation is Active, and Stop neutralizes
the state. The method cannot carry network coordinates, a Route, peer identity,
permission, path or executable text. This is local input admission, not peer
authorization or sharing consent.

ANDROID-A01D3 admits only a complete touch-contact string of at most 160
characters. It contains zero to five canonical unsigned integer tuples; contact
IDs must be unique and coordinates/pressure remain within fixed normalized
bounds. No endpoint, peer, Route, path, command name or executable payload is
accepted. The Service submits frames only for the observed Active generation and
neutralizes on Stop. Browser-frame coalescing limits ordinary MOVE pressure, but
the future data-plane worker still owns transport rate limits and peer admission.

ANDROID-A01D4 exposes only the fixed Android microphone and speaker action IDs
with lifecycle-valid start/retry/stop operations. The WebView cannot grant a
permission: Start returns to the Activity main thread and Android owns the
existing recording/notification prompt. The read DTO contains only bounded
lifecycle, format, frame-count, permission-ready and problem-code fields; it
contains no PCM, endpoint secret, launch argument, path or peer authority.

ANDROID-A01D5 admits sharing-selection mutations only for an exact capability
ID in the Service's current catalog. The capability must have present hardware,
an attached Adapter and a Source Port. Unknown/foreign IDs, overlong IDs,
unattached entries and Sink capabilities are rejected. A01E attaches camera, so
its present Source becomes eligible without bypassing this rule. This session-local
set is publication intent only and conveys no Session trust, Route authority,
permission grant or transport endpoint.

The portable control-host publication projection independently revalidates a
host-supplied Source selection against the current Core catalog. Unknown IDs,
Sink/Control-only Capabilities and Offline Sources are rejected before encoding.
Unselected Source-bearing Capabilities are removed, Adapter ownership is pruned
to the retained catalog and the result is validated again. This prevents stale
or forged UI selection from expanding an outbound CatalogSnapshot; it does not
authenticate the recipient or authorize a Route.
The outbound operation obtains its Session ID from the same exact peer binding
used for inbound admission; the platform caller supplies only the current local
catalog and publication selection, not a second Session identifier.
The Windows Tauri selection command uses the same projection transactionally:
it commits the candidate set only after projection succeeds. Removing a stale
selection is allowed after the corresponding Source becomes Offline, preventing
an unavailable device from becoming permanently selected.
Android applies the equivalent fail-closed rule in its Service-owned selection
boundary and pure local publication projector. The projector rechecks every
selected ID against the current catalog, preserves only unselected
Sink/Control-only inventory and refuses an Offline, detached, absent or
non-Source selection. Candidate state commits only after this complete
projection succeeds; deselection remains possible after the selected Source
goes Offline. No recipient, Session or endpoint is accepted from the WebView.

ANDROID-A01E accepts only the fixed camera action ID and lifecycle-valid
start/stop/retry. Start is returned to the visible Activity so Android owns the
CAMERA/notification prompts; the WebView cannot grant consent. The unexported
foreground service owns Camera2 and the privacy notification. The default
export endpoint is fixed by the facade and cannot be supplied through the UI
command. Stop closes exporter, codec and camera resources and removes the
foreground service.

ANDROID-A01H adds only the fixed haptics lifecycle action and one bounded Binder
command. Magnitudes are restricted to 0..1000, non-zero rumble duration to
1..10000 ms, and the generation must match the Active Sink. Stop carries no
strength or duration. The single-actuator projection accepts no peer, endpoint,
Route, path or text payload; authorization remains mandatory before a future
network worker may call it.

ANDROID-A01E2 exposes no arbitrary Camera2 request fields. Refresh uses the
read-only bounded inventory only after platform consent. Source mutation accepts
an empty automatic token or an exact key from that session's cached inventory;
quality accepts three fixed names. Both fail while capture owns resources. The
DTO contains IDs, facing, bounded Zoom targets and lifecycle only—no pixels,
access units, peer address or authorization secret.

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

ADR 1106 permits only the first explicitly authorized physical touchpad lab to
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

The first Windows virtual-camera plan is current-user and session-only. Frame
payloads never enter JSON-RPC, names and buffers are bounded, NV12 padding is
cleared before publication, and sample sequence/time must advance within one
stream epoch. The registrar must retain an exact symbolic link after
successful start and pair every start with stop/shutdown rollback. Creating or
registering the Projection remains a separately approved system mutation.
The pre-COM media-source core admits at most four FIFO sample requests and
cancels them on stop/shutdown. Registration orchestration exposes failed
rollback as `CleanupRequired`; it never silently reports a failed cleanup as a
stopped camera.
The in-process COM projection keeps source and stream event queues separate,
uses non-blocking request-path locks, retains only deterministic fixture frames,
uses the Frame Server-provided bounded allocator and clears 2D-buffer padding
before publishing a sample. Sample-construction failure removes its accepted
request ticket and publishes `MEError` without changing system camera state.
The class-activation boundary remains fixed to one source CLSID, returns
`IMFActivate`, rejects COM aggregation and tracks outstanding objects/server
locks before allowing DLL unload. The real virtual-camera backend cannot
represent system lifetime,
all-user access, arbitrary categories, registry additions, device properties or
physical camera dependencies. The controlled executable accepts no paths,
CLSID, names or access modes from the command line. Any HKLM COM deployment is
an explicit local-lab operation against one hash-recorded DLL and must be
removed after the session test. The recorded 001B1C host run followed that
boundary and verified the DLL, CLSID and empty lab directories were absent
after rollback.
The 001B1D sharing check accepts no child executable, count, camera identity or
deadline on its command line. It resolves only its current hash-recorded
executable and sequentially spawns exactly two `consumer-probe` children
directly without a shell. The parent passes only its just-started camera's exact
symbolic link through a child-only environment value. The child bounds that
value to 4096 UTF-16 units, rejects control characters and rejects every shape
except the Windows software virtual-camera prefix before performing an exact,
case-insensitive enumeration match. Each child has a separate 20-second
deadline; error and timeout paths kill and reap it before registrar
stop/shutdown. The internal probe entry point is a controlled lab surface, not
an authentication boundary. This demonstrates bounded sequential local
cross-process consumption only; it grants no peer trust, does not establish
simultaneous fan-out and carries no frame payload over process stdout or the
control plane.

The 001B1E decoded-frame ingress accepts only an owned payload matching the
canonical 1280x720 NV12 size. It binds one typed stream ID and positive epoch,
requires strictly advancing sequence and source timestamp, rejects end-of-
stream and bounds its queue to at most twelve frames. Overflow drops the oldest
frame and marks the next retained sample discontinuous instead of growing
memory. The Media Foundation request path uses `try_lock`; an empty or contended
ingress returns `MF_E_NOTACCEPTING` rather than blocking Frame Server. This seam
is process-local and non-registering. It does not name, create or trust a
shared-memory object and cannot yet receive a payload from the Runtime, network
or Android device.

CAPY-CAMERA-001B1F introduces one fixed local IPC object for decoded frames:
`Global\\CapyIO.CameraIngress.v1`. Its protected DACL grants full access only to
SYSTEM, administrators and the mapping owner, and grants LocalService generic
read only (`D:P(A;;GA;;;SY)(A;;GR;;;LS)(A;;GA;;;BA)(A;;GA;;;OW)`). Consumers
request only `FILE_MAP_READ`; a second producer fails when the named mapping
already exists. The mapping has a versioned 4,147,648-byte layout, a non-zero
generation, bound stream ID/epoch and exactly three canonical NV12 slots. A
consumer validates every immutable header field and uses a per-slot commit
marker before and after its bounded copy, rejecting stale, malformed,
regressing or concurrently replaced data. Missing publications are observable
as discontinuity. The generation is freshness evidence, not a secret or peer
authentication token; OS ACLs are the only trust control in this local slice.
The object is ephemeral and disappears after its last handle closes. B1F does
not connect the mapping to Runtime/network/Android input and does not change the
registered fixture-backed class factory, so no new system camera state is
created by its normal tests.

B1G reuses the same mapping and ACL without adding a control or payload input.
Its test parent passes only a unique bounded `Local\\...test...` mapping name to
one directly spawned copy of the current test executable. The child opens that
mapping through the read-only consumer API, constructs only a non-registered MF
source, validates one sample and shuts it down. No shell, external executable,
global production object, registry key or virtual-camera API is involved. This
is process-isolation evidence, not a production authentication mechanism.

B1H moves all raw mapping access into `capyio-windows-camera-share`; the COM
crate no longer owns producer handles or calls file-mapping/security APIs. Its
optional cross-crate test surface accepts only bounded
`Local\\CapyIO.CameraIngress.v1.test.*` names and is disabled in production.
The headless camera-host layer owns one producer, rejects duplicate start,
rejects publish while stopped and drops the mapping on explicit stop or host
drop. Failed duplicate ownership leaves the second host stopped and retryable.
The read side may adopt stream identity/epoch only from the already ACL-
protected and fully validated production header; this is local OS trust, not
remote peer authorization. No listener, decoder, Android permission or camera
registration is added by B1H.

The separately authorized CAPY-CAMERA-001C0 Android lab declares exactly
`android.permission.CAMERA`. It requests permission only after the user presses
the visible start button, never resumes capture implicitly and closes its
Camera2 device when the Activity pauses or its preview surface is destroyed.
The window sets `FLAG_SECURE`; backup/data-transfer rules exclude application
data; the manifest declares no Internet, storage, microphone, location or
foreground-service permission and no service component. The two-image
`ImageReader` acquires only the latest image and closes it synchronously. The
lab neither stores nor transmits pixels and contains no network listener.
Repository validation pins this exact permission/service boundary.

One explicitly authorized, exact-target device run installed the hash-recorded
APK. CAMERA was initially denied, the user granted the visible system prompt,
and Camera Service later reported no active client after the Activity lost
visibility. No preview image or wireless-debugging endpoint was retained in
repository evidence. APK removal and any future permission/service change
remain separately controlled operations.

CAPY-CAMERA-001C1 adds no permission or component. Its MediaCodec callback has
no network, file, UI or ordinary logging path. It accepts at most a 4 MiB codec
buffer, performs one bounded owned copy, attempts a non-waiting queue lock and
always releases the codec output. Queue capacity is at most eight; contention
and full-queue loss increment an observable drop count. Codec parameter sets
are capped at 64 KiB. This controls local memory/work but supplies no peer
authentication, encryption, replay protection or wire framing. C2 subsequently
attached this boundary to Camera2 and exercised a vendor encoder on the approved
device.

CAPY-CAMERA-001C2 connects the encoder only as a second Camera2 output beside
the visible preview. It adds no permission, service, listener, file path or
transport. The Activity displays captured/encoded/drop counters but never
pixel or access-unit bytes. Each camera result drains at most the queue's fixed
eight-entry maximum, so a malicious or faulty codec cannot create an unbounded
UI handoff loop. The authorized device run produced bounded vendor AVC output
and pause-driven release; it did not export bytes or enable a listener.

CAPY-CAMERA-001C3 adds a parser/record contract, not transport security. Every
record has an exact maximum size, fixed version/header, non-zero stream ID,
positive epoch and closed kind/flag/layout values. The Rust side validates the
entire record before allocating an owned bounded payload, and its transactional
guard rejects wrong-stream/epoch input, replay, timestamp regression, unmarked
gaps, non-key discontinuities and post-EOS data. Java and Rust share fixed
goldens. No socket, INTERNET permission, service, file or log path is added.
The record must remain unreachable from untrusted peers until an authenticated,
confidential Route/Session-bound transport with its own replay window is
reviewed.

CAPY-CAMERA-001C4 declares the separately authorized normal Android
`android.permission.INTERNET` permission but no service or additional
component. Its experimental exporter has one hard-coded destination: device
loopback port 38173. It neither discovers peers nor accepts inbound sockets.
Only a separately established, exact-device ADB reverse mapping can bridge that
connection to the Windows receiver, which binds `127.0.0.1` and rejects a
non-loopback peer. This inherits the authenticated Android-debugging session's
local-lab trust; CAVC still supplies no authentication or confidentiality and
the path is forbidden as a production or untrusted-network transport. Both
codec and camera callbacks remain free of socket operations. A fixed queue
drops rather than waits, receiver reads have a 15-second deadline, and record
size/stream/epoch/sequence checks run before any downstream decoder. As of this
slice, the exact V2419A target accepted the hash-recorded APK after explicit
authorization of CAMERA, INTERNET and the 38173 ADB reverse mapping. The run
delivered 90 guard-accepted vendor access units and four key frames, then the
app was force-stopped. Camera Service reported no active client and the reverse
mapping was removed. The installed app remains present and force-stopped with
INTERNET granted; no background service exists to use it.

CAPY-CAMERA-001C23 adds a second, explicit trusted-lab transport choice rather
than weakening the C4 default. The Android address field is not persisted and
blank still means device loopback. Non-empty input accepts only canonical
IPv4 literals in RFC1918, link-local or 100.64.0.0/10 space; it rejects DNS,
IPv6, public, loopback, multicast, wildcard and host-with-port input. The
Windows receiver requires `--trusted-lan-bind` and `--trusted-lan-peer`
together, binds exactly one interface on fixed port 38173 and admits only the
exact IPv4 peer. The Android side remains outbound-only, visible and bounded by
the existing pause/close lifecycle and retry policy. No firewall rule,
discovery record, credential, background component or new permission is added.

These restrictions reduce accidental exposure and cross-peer injection, but
they do not provide application identity, confidentiality, integrity or replay
protection. A compromised allowed peer can still send arbitrary bounded CAVC,
and an observer on an ordinary LAN can inspect plaintext. A Tailscale address
may protect overlay transit but still does not authorize a CapyIO Route. C23 is
therefore forbidden on an untrusted network and cannot be described as the
production no-ADB transport; that requires the production controls listed
under **Eavesdropping, injection and replay**.

CAPY-CAMERA-001C24 lengthens only bounded foreground-lab timing. Android makes
at most 120 connection attempts with the existing 500 ms connect and retry
delays, and Windows bounds initial mapping readiness to 120 seconds plus
post-connection recovery to 60 seconds. The Activity's keep-screen-on flag is
cleared with its active state; pause still closes capture and no service or
background permission exists. Cleanup errors from the child or Global mapping
now take precedence over an earlier validation error, so stale system state is
never reported as an ordinary receiver failure. These controls improve lab
recovery but add no peer identity, authorization or transport security.

CAPY-CAMERA-001C25 treats rotation as a configuration-only event only while the
same Activity remains visible. It retains the existing Camera2 session and
in-memory endpoint but does not persist either across real Activity/process
destruction. The unchanged `onPause` and preview-surface-loss paths still close
Camera2, MediaCodec and the exporter, and the keep-screen-on flag remains tied
to active visible state. The Activity does not lock orientation and no service,
background permission, new network authority or payload retention is added.

CAPY-CAMERA-001C5 keeps decoding in the Windows user-mode Adapter after the C3
record and replay guard. It accepts Annex-B only, caps decoded NV12 at 32 MiB,
allows at most 64 unmatched samples, at most 16 outputs per ordinary drain and
two consecutive output-type changes. SPS/PPS are copied only into the bounded
first key-frame sample after start/discontinuity. The lab computes checksums and
drops decoded pixels without file output, preview capture or ordinary payload
logging. It does not publish shared memory, activate/register a virtual camera,
change a driver or broaden the loopback/ADB trust boundary.

CAPY-CAMERA-001C6 keeps production and lab object namespaces distinct. The
ordinary Adapter may use only the compile-time enabled exact Local lab name;
the COM class factory ignores it. Registered activation opens only
`Global\CapyIO.CameraIngress.v1`, accepts its fixed ACL/header/identity layout,
falls back to the fixture solely for Win32 file-not-found and fails closed on
access denial or malformed state. A non-elevated preflight received error 5
when creating the Global mapping, confirming that production ownership belongs
in a separately authorized privileged host/service. No code attempts automatic
elevation, weakens the DACL or registers a camera in this slice.

CAPY-CAMERA-001C7 adds no trust principal or writable IPC surface. Only the
already validated read-only Global consumer enters the asynchronous sample
pump. At most four accepted requests retain optional COM tokens, all mutable
queue state is serialized on a Media Foundation work queue, and a 5 ms timer is
scheduled only while a bounded request awaits a newer publication. Stop,
shutdown, scheduling failure and real sample errors release pending tokens and
reservations. The controlled administrator deployment remained hash-locked to
one DLL and fixed CLSID; final rollback removed the Session registration, CLSID,
DLL and empty directories without changing drivers, privacy, boot or security
policy.

CAPY-CAMERA-001C26 adds no mapping name, write authority or caller-controlled
probe surface. Only registered activation may enter its late-bound state, and
production checks continue to target exactly
`Global\CapyIO.CameraIngress.v1`. Exact file-not-found permits the deterministic
offline fixture; access denial, malformed headers and every other open/read
failure remain terminal. Mapping checks run only on the existing Media
Foundation serial work queue after a fixed placeholder-frame countdown, never
in a network/codec callback. The first live frame is marked discontinuous and
rebased to the already active virtual-camera timeline; placeholder data cannot
reappear inside that live stream. Local names are available only to the
feature-gated test constructor. No registration, service, permission, peer
identity or transport-security authority is added.

CAPY-CAMERA-001C27 adds no writer, name, trust principal or timing thread. It
applies a fixed empty-read counter only inside the existing registered source's
bounded serial sample pump. On expiry it releases its read-only handle before
returning to locally generated placeholder frames; it never deletes or mutates
the mapping. Reattachment still opens only the fixed production name and
validates the protected ABI/header before reading. A remembered
generation/stream/epoch/sequence/timestamp tuple rejects stale replay after a
mapping is reopened, while a new generation may start a new source timeline and
is rebased onto the existing virtual output. Access denial, malformed state and
non-monotonic input remain terminal. The feature-gated Local target remains
test-only. No registration, elevation, service, permission, peer identity or
transport-security authority is added.

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

### Android audio shell

ADR 0043 requires a visible user action plus granted recording and notification
permission before the 001C microphone Source starts. The non-exported service
uses the microphone/media-playback foreground types, a persistent notification
with Stop action and `START_NOT_STICKY`; Activity closure is not implicit
authorization to restart capture.

The 001C APK declares no Internet permission, disables cleartext traffic and
backup, never stores or logs microphone bytes and exposes only an app-private
Node UUID. Captured bytes are counted then discarded. This is a local privacy
boundary, not transport security. Permission revoke, indicator, lock/background
and vendor process-management behavior still require physical evidence before
release claims.

## Milestones

1. Foundation: deterministic validation and process boundaries.
2. Local real-Adapter lab: clearly marked insecure/local mode.
3. Pairing spike: device identity and authenticated transcript.
4. Production transport: encryption, replay and downgrade binding.
5. Projection/driver hardening: ACLs, isolated tests, fuzzing and review.
6. Release: signing, SBOM, vulnerability and update process.

No build before milestone 4 is described as safe for untrusted networks.
