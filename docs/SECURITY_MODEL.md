# CapyIO Security Model

ADR 1511 adds explicit service allocation for ordinary-user camera shared memory.
The service handles allocation only; user processes retain media and camera lifetime.
The ordinary-user path passed local-lab media validation on 2026-10-04 with the
explicitly configured service (see ALPHA_GAP_CLOSEOUT_20261003.md). Previously
packaged installers still require separate service deployment; this is not a
clean-machine or production installation claim.

ADR 1509 separates Windows UI from the Node Host. The local API uses exact-user
SID DACLs, per-login-session pipe names, remote-client rejection and first-instance
creation. Clients verify the server executable path before sending data. Requests
are versioned and dispatch only the 53 existing typed operations plus Host status/
shutdown; frames are limited to 1 MiB and four fixed workers serve requests.
Node API reads are explicitly nonblocking with time bounds; response acknowledgement
avoids unbounded pipe flush waits. Existing service pipes retain their old limits.
No elevation or low-integrity ACL is added. Same-user arbitrary-code compromise is
not isolated by this API. A UI from another install must exit the old Host first.
UI close does not revoke active Routes; explicit node exit runs normal cleanup.

ADR 1507's opt-in enrollment recovery reuses the exact approved protected key and
peer record. A pending journal is not an active control grant; normal enrollment
commits only after both peers report durable preparation. Explicit local recovery
never replaces a different existing peer record and does not assert remote completion.

ADR 1506 adds an explicitly opened local-alpha first-enrollment channel, separate
from control TLS. An eight-digit one-attempt PIN authenticates ordered public
Node/certificate hellos through the pinned SPAKE2 implementation; directional
confirmation is followed by ordinary exact-leaf mTLS and bilateral user consent.
Only locally generated private keys enter native DPAPI/Keystore storage. Existing
pins are never overwritten by this entry. No hardware permission is granted by
pairing. The dependency is not independently audited; durable interrupted-commit
recovery and full physical acceptance remain open, so this is not production
enrollment. See ADR 1506 and its dependency note before extending this boundary.

ADR 1505's display workflow reuses paired control TLS for bounded launch/ACK
material. This never goes through WebView or diagnostic logs. Local resource
consent remains independent at both endpoints. The requester-SID Windows service
owns the worker, accepts only typed parameters and keeps a finite renewable
generation-bound deadline. No node private key is copied to LocalSystem. Ready
means worker-start acknowledgement; Active requires receiver frame evidence.
This does not broaden the existing GameStream adapter's media security claims.

ADR 1503 binds the existing C23 camera lab worker to an authenticated control
Route with resource consent, endpoint leases, expected Stream/epoch and finite
child lifetime. The control owner stops capture/receiver/projection on revocation
or disconnect; only published decoded frames mark Windows Active. The old fixed
camera broker pipe, driver boundary and trusted-lab IP restrictions are unchanged.
This does not authenticate or encrypt CAVC records; production network use remains
out of scope. No new Android permission is added by this handoff.

ADR 1484's Windows inbound listener is scoped to an explicitly selected active
paired Node and local IP/port. It loads the current user's DPAPI credential,
pins the exact peer leaf, checks the fresh Session binding and does not treat
the TCP source address as trust. Rejected handshakes do not admit a peer or
grant a Route. The Android WebView sees only endpoint hints and bounded status,
never private keys or credential bundles.

The Windows current-account credential check only attempts DPAPI decryption
of the selected active peer's bundle and returns a bounded ready/error result;
it never serializes key material to the WebView. A public pairing record is
not evidence that the current process token owns the protected credential.

ADR 1485 requires old-credential DPAPI decryption before replacing an
existing paired record. The renewed invitation is written before committing
new trust metadata and exact new artifacts are rolled back on failure. No
certificate observed on the network can trigger renewal or overwrite a pin.

ADR 1473 binds a separate touchpad TLS 1.3 connection to the exact paired leaf,
Session, Route, Source/Sink, Stream, epoch and finite grant expiry. Endpoint
offers may change only the port on the authenticated control peer's address.
Both hosts recheck Runtime authorization/leases; the Windows receiver opens its
native Sink only after admission and Hello validation. Stop interrupts bounded
I/O and closes the Sink. No plaintext LAN fallback, driver ACL change, new
Android permission, or automatic elevation is introduced.

ADR 1448 makes the stored peer leaf-certificate fingerprint mandatory in both
control TLS configurations. After normal mTLS/ALPN verification, the channel
must match the exact leaf before sending or accepting application binding
records. A shared issuing CA alone is not a paired identity. Rotation is never
accepted automatically from discovery or an observed replacement certificate.

The ADR 1446 discovered-device UI exposes only bounded, expiring observations
from the existing ingress. Node IDs and endpoints remain unauthenticated hints;
they cannot invoke pairing, connect, grant a resource or start a worker. No
broadcast-provided name or HTML is rendered, and no nonce or credential reaches
the DTO. Candidates remain visually separate from admitted resource catalogs.

ADR 1454 appends only the local paired-directory classification to that DTO.
`paired` is not a claim about the broadcast sender: the endpoint must still
present the stored exact leaf certificate and complete the expected Node/Session
binding. `revoked` never becomes connectable, and no handle, fingerprint or
key material is disclosed to the WebView.

The ADR 1443 Windows discovery interface command is parameter-free and read-only.
It returns at most 64 local IPv4/index pairs from bounded IP Helper output,
never credentials, MAC addresses, gateways or DNS settings. Starting a listener
rechecks address membership. Enumeration supplies neither peer trust nor network
reachability and does not change firewall or adapter configuration.

The Android saved-audio-peer readback bridge exposes only a versioned,
direction-scoped private/overlay IPv4 and UDP-port DTO. It does not expose
credentials, arbitrary preferences or effective build configuration. The UI
validates exact keys and bounds; reads never start hardware or establish trust.
See ADR 1431.

Android display credentials are imported only through the system document
picker into a native, fixed-schema bundle parser. The WebView receives no URI,
path or credential bytes. Archives are bounded by entry count, exact root names,
per-entry sizes and total uncompressed size; paths, symbolic links, duplicates,
unknown content and schema/transport mismatches fail closed. Installation uses
only fixed app-private managed directories and preserves the prior configuration
until staging completes. Import does not establish trust: certificate/key
parsing, peer authentication and Route binding remain later gates.

Android GameStream expectation properties are derived by the Windows service's
strict schema-3 parser rather than duplicated by hand. The metadata-only
converter substitutes a fixed placeholder for credential reads and emits no
server key or certificate bytes. The bundle writer subsequently receives an
explicit client credential set; its ZIP output contains a private key and must
be transferred and deleted according to credential-handling policy. Public
device storage is not an approved implicit transfer channel.

The display receiver bridge (ADR 1424) exposes only a bounded prerequisite DTO
and one fixed unexported Activity. It accepts no paths, credentials or launch
arguments from web content. File presence and library load are not trust or
media-readiness evidence; full validation and mutual authentication stay at the
receiver transport boundary. Missing GameStream code cannot silently fall back
to a different transport.

Runtime audio lab settings (ADR 1423) accept only canonical private/overlay
IPv4 literals and explicit per-direction UDP ports after visible lab-risk
acknowledgement. Settings are app-private and immutable while the selected
Adapter has an active worker. They retain existing fixed lab stream bindings;
they are not pairing, authentication, encryption or replay-safe production
authority. Neither saving a setting nor successful media reception establishes
trusted peer identity. Public-network audio remains outside this lab contract.

> Status: pre-alpha threat model. A generic mTLS control boundary and bounded
> post-pairing records exist; production enrollment, secure platform storage,
> rotation, revocation propagation and recovery are not complete.

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
CatalogSnapshot parsing now also rejects excessive Adapter, Capability, Port,
format, metadata, protocol-version and ownership-reference counts, plus any
catalog string larger than 2 KiB, under the existing 1 MiB Envelope ceiling.
The canonical outbound helper applies the same limits. This bounds semantic
work and retained allocations after Protobuf decode; it does not authenticate
the sender or turn the future transport binding into a credential.

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

ADR 1501 is an accepted, opt-in exception only on a separate SID-scoped native
audio Route pipe, never on ADR 0034's all-interactive-user pipe. Administrator
launch configuration chooses the principal and pinned native worker executables.
That principal's Node Runtime is trusted to check authenticated Session, resource
consent, catalog and leases; caller-supplied Route JSON is not cryptographic proof.
The service checks direction, fixed PCM preset, binding, finite grant and endpoints,
and bounds owner liveness with a 15-second renewable monotonic lease. Requests
cannot change executables, SIDs, devices or persistent configuration. Dynamic
endpoints belong only to one worker launch. Legacy operations cannot change a
Route-owned worker. The desktop handoff is restricted to the explicitly approved
Windows/Y700 lab endpoints; it does not encrypt native UDP or claim production
security. Broader endpoint policy and real-device validation remain release work.

ADR 1426 adds an opt-in display configuration path only to trusted service
launch arguments, not to that interactive-user pipe. The existing bounded,
closed display configuration loader and hash-verified worker boundary remain
mandatory. Display startup runs on its own thread, has a fixed cancellation
deadline and cannot signal audio/service shutdown. Default service launches
do not enable it. This source integration neither deploys the configuration
nor grants driver installation, peer enrollment or UI-supplied launch authority.

ADR 1427 optionally enables a separate display pipe for a Windows-validated SID
supplied through administrator-controlled launch configuration. Its ACL includes
that principal, administrators and System; it does not reuse audio's all-
interactive-user grant. First-instance ownership and remote-client rejection
are mandatory. Closed 4 KiB requests contain only version, correlation and
Status/Start/Stop, and cannot replace the trusted display configuration.
Tauri commands accept no arguments and run IPC outside the UI thread. No
automatic elevation, service installation or display enrollment occurs.

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
service configuration and cannot be supplied through the legacy control pipe or
WebView. ADR 1501's separate principal-bound handoff is the sole dynamic Route
exception described above. The legacy child accepts one fixed common-packet binding and explicit
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
Android descendant catalog UUIDs are deterministic UUIDv5 values namespaced by
the app-private persisted Node UUID and domain-separated for Adapter,
Capability and Port identities. Platform keys are locally authored, canonical
and bounded before hashing. UUIDv5's SHA-1 digest is not used for secrecy,
signing or peer authentication; every derived ID remains public and still
requires Session and Route authorization.
The Android-to-Core catalog projector accepts only closed enums, bounded
collections and canonical platform keys. It rejects unattached Capabilities
unless they are Offline and delegates final ownership/Profile/Port validation
to Core. QoS, interoperability and supported Route modes are explicit DTO
fields rather than values inferred from UI text. This prevents a future JNI or
transport boundary from silently upgrading a private Adapter contract.
The Java-to-Rust schema-v1 JSON handoff is limited to 64 KiB and denies unknown
fields at every object level. Its Java policy is a closed switch over canonical
Capability keys: current private vertical paths may declare Adapter-managed
interoperability, while capabilities without a remote path declare only a local
pipeline. The Binder accepts no recipient, Session, endpoint or caller-selected
Route backend, so producing this catalog cannot authorize or deliver it.
The ADR 1406 JNI owner uses opaque positive process-local handles guarded by a
mutex with at most four live owners, rejects unknown/closed handles, catches panics at every new JNI entry,
and refuses a disconnected catalog replacement that changes Node identity.
The exposed inspection is only a bounded capability count. No peer, Session,
endpoint, Route or credential is accepted by this JNI surface.
ADR 1407 binds that handle to the unexported Service lifecycle and requires a
candidate sharing projection to cross native validation before Java selection
commits. Packaging the library grants no new endpoint or network authority.

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

The unified touchpad lab sender closes the owned socket on Stop, clears only
that generation's queue and removes its JNI record session on the worker even
after transport failure. It refreshes held contacts at 250 ms intervals. While
contacts are active, the lab receiver uses a two-second socket read timeout;
record-boundary EOF explicitly disconnects its Sink and malformed/partial records
retain fail-closed Drop cleanup. These are lifecycle safeguards, not Route
authorization, authentication or encryption. The loopback/ADB boundary remains
unchanged; direct LAN input is not enabled by this change.

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
ADR 1418 gives a future dedicated camera Broker a separate fixed local pipe.
Remote clients are rejected; its DACL admits only LocalSystem, administrators
and the Windows-validated SID of the ordinary process that requested elevation.
The closed 4 KiB protocol contains only Status/Start/Stop, version and request
correlation. It conveys no executable, path, argument, mapping name, camera
identity or transport endpoint. Partial
Start is rolled back and rollback failure is explicit. This contract grants no
service installation, elevation, COM registration or peer/network authority.
ADR 1419 permits one fixed elevation transition: the ordinary Windows process
may ask ShellExecute `runas` to start its current executable with exactly
`--camera-broker-host` and its own process-token SID. That mode validates its
elevated token and asks Windows to parse the bounded SID before opening the ADR
1418 pipe, then branches before any Tauri window or WebView exists. No UI value
contributes to the executable, verb, mode or SID. ADR 1420 rejects malformed
and injection-shaped SID text and avoids granting all interactive users merely
to accommodate cross-account UAC. The host owns only the fixed camera session,
receiver and exact ADB mapping; successful Stop closes the projection-scoped
host. This does not weaken the shared-memory ACL, install a service or driver,
or grant network/peer authorization.
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

LAN discovery candidates are unauthenticated hints. ADR 1432 deliberately
excludes names, catalogs, credentials and IP addresses from its fixed record;
the receiver obtains an address from packet metadata and must authenticate the
subsequent control connection. A candidate Node ID or nonce grants no peer,
Session, Capability or Route authority and must never be rendered as paired.
The shared ingress admits at most 256 discovery datagrams per monotonic
one-second window, counting malformed traffic toward the same budget; it still
opens no socket and grants no authority.
ADR 1439 fixes the optional socket host to one caller-reviewed IPv4 multicast
membership, administratively scoped group, TTL 1 and the existing bounded
ingress. Portable reception uses a wildcard IPv4 socket on one fixed
dynamic/private port, so cross-interface unicast remains possible but gains no
trust. The host is not automatically started and cannot choose another port
after bind failure.

The shared UI preferred-peer profile from ADR 1433 contains only one canonical
private/overlay IPv4. It never stores tokens, credentials, ports or grants and
is a form suggestion rather than identity evidence. Native per-feature settings
remain authoritative; saving or clearing the profile causes no network or
hardware operation.

ADR 1434 makes Port lease authority local to the Node that owns the Port. A
claim is not inferred from catalog/UI state: it binds an already-authorized
Route and Ready Session, has a finite expiry and cannot outlive finite Route
authorization. Expiry invalidates the Route epoch. ADR 1435 adds the bounded
remote command shape and a transport-agnostic admitted-host handler, but not a
socket or authenticator. The host binds the request to one exact Ready Session,
peer, Route and owner-local Port; malformed, cross-Session, non-local and replayed
requests fail closed. A production caller must still establish the authenticated
control binding and cryptographic replay protection before invoking it.

ADR 1436 provides a generic mTLS 1.3 control channel with early data and server
session tickets disabled, a dedicated ALPN, mutual certificate verification,
fixed Session/server/client Node binding, bounded deadlines and a 1 MiB frame
ceiling. Its CA inputs must represent the already-paired expected peer; a broad
ambient trust store is not an authorization policy. The crate does not create,
store, rotate or revoke credentials and therefore does not complete pairing.

ADR 1455 permits fresh Session agreement only inside that authenticated and
leaf-pinned TLS channel. The client supplies a random non-nil Session ID; the
server accepts it only with exact preconfigured server/client Node IDs and
canonical reserved bytes, then echoes the complete binding. A changed Node is
rejected after mTLS. The platform owner must prevent reuse among active peer
Sessions and close on failed Runtime admission; the identifier itself is not a
bearer grant and supplies no hardware authority.

ADR 1456 admits that binding into Runtime transactionally. The only pre-catalog
peer metadata is the already-paired display label plus an explicit pending
platform marker; Adapter and Capability maps are empty. Local/remote IDs come
from the TLS binding, and an existing peer is rejected rather than implicitly
reviving its suspended Sessions. Any failure drops the channel and leaves the
original Runtime unchanged.

Disconnect cleanup removes transport ownership before Runtime mutation, closes
only the exact bound Session, invalidates its live Route epochs and releases its
leases. Another live Session for the same peer prevents the global peer-presence
flag from being cleared. Repeated cleanup is idempotent and cannot resurrect a
closed Session.

ADR 1471 carries Route and lease operations on that same authenticated link.
ADR 1472 supersedes its publication-only authorization for an Android-local
Touchpad Source: a bounded 60-second request requires explicit local Allow,
produces a Route-specific finite grant, and fails closed
on denial, expiry, disconnection or loss of publication. Pending responses grant
neither local mirror authority nor an owner lease. Other alpha capability
policies are unchanged. This is resource consent, not online first-device
pairing, remembered trust, transport configuration or physical input evidence.
An incoming Route proposal must be Draft/Pending, use the exact bound Session
and resolve one endpoint on each admitted Node. Android's alpha authorization
policy is limited to its current publication-filtered catalog. The Port owner
sets an exclusive policy and is the only side that can grant the finite lease;
Start fails without a non-expired owner-local claim. Catalog visibility,
authentication and Route authorization remain distinct decisions. The
five-minute Port lease remains independent of authorization. ADR 1486 now
extends explicit touchpad consent to at most eight hours on the same Route and
Ready Session. Its control owner renews five-minute leases every 60 seconds,
bounded by the original grant; data workers cannot extend authority. Missing
or expired claims, denied renewal, Stop and Session loss close the data path.
No approval persists across a new Route or reconnection. Route control does
not imply that a feature Adapter's separate data endpoint was configured.

ADR 1457 exposes only a Node ID in the Windows WebView command. Endpoint,
server name, exact leaf fingerprint and credential handle are re-derived from a
fresh discovery observation plus active paired record; private material is
decoded only through the typed DPAPI path. A link is not counted connected until
the peer's bound catalog is decoded and atomically admitted. Failure at send,
receive or apply closes the exact Session, and the fixed deadline prevents an
unbounded UI-request network wait.

ADR 1437 separates discovery hints from post-pairing trust metadata. The
directory retains at most 64 peers and stores only Node identity, bounded UI
label, TLS server name, SHA-256 peer-certificate fingerprint and an opaque
platform credential handle. It contains no private key or trust-root bytes.
Only a fresh exact Node match in Active state produces a connection candidate;
unpaired, stale and revoked candidates cannot. This classification is still
not authentication: a forged discovery Node ID must fail the subsequent mTLS
certificate and Session/Node binding.

ADR 1441 permits persistence only of the bounded non-secret paired-peer
directory. The schema contains certificate fingerprints and opaque credential
handles, not certificate DER, trust roots, private keys or pairing codes.
Unknown fields, duplicate identities and malformed state fail closed. Copying
the JSON cannot substitute for access to the referenced platform credential.

ADR 1442 uses current-user Windows DPAPI plus fixed CapyIO optional entropy and
forbids DPAPI UI. Plaintext and protected inputs are bounded, errors contain no
credential bytes, and the DPAPI-owned plaintext allocation is cleared before
release. Machine scope is not used. File placement, ACLs, deletion and rotation
remain pending and must not be inferred from the protection primitive alone.

ADR 1449 adds fixed current-user file placement and atomic replacement around
that primitive. Credential handles are a closed prefix plus 64 lowercase hex
characters and never become caller-selected paths. Reads are bounded; malformed
or wrong-owner paired directories fail closed instead of becoming empty trust
state. The directory inherits the current-user local-app-data ACL and credential
blobs remain DPAPI protected; no custom hardened ACL is claimed. Credential-
first, directory-last ordering prevents the supported commit path from
publishing a reference before its blob write, but external deletion and orphan
cleanup remain connection-manager/recovery concerns. Store readiness neither
authenticates a peer nor proves that every referenced credential is present.

ADR 1450 provides the Android counterpart for credential bytes. A closed
64-lowercase-hex handle selects a non-exportable Android Keystore AES-256 key
and a no-backup app-private envelope. AES-GCM uses a fresh 12-byte IV;
`AtomicFile`, a 64-KiB plaintext ceiling and bounded envelope parsing precede
decryption. Key absence, malformed input and tag failure are terminal. Removal
deletes the key before ciphertext, so interruption may leave only unreadable
bytes. The store is not a pairing transcript, trust decision, WebView API or
authenticated transport; paired-directory persistence is separate in ADR 1451.

ADR 1451 places non-secret Android trust metadata in a no-backup `AtomicFile`
while retaining the shared Rust parser as authority through bounded byte-array
JNI. Unknown fields, self-pairing, duplicate peers, invalid handles/fingerprints
and oversized input fail before replacement; corrupt stored content never
becomes an empty trust state. The Service has not admitted this store, so the
primitive itself grants no peer access and performs no credential lookup.

ADR 1452 lets the non-exported Android Service retain only a Rust-canonicalized
document and expose its state/count through the existing local Binder. ADR 1459
requires it to resolve every active record's Android-Keystore handle and
revalidate the typed control bundle before reporting `ready`; temporary
plaintext arrays are overwritten after validation. Revoked records never cause
credential recovery. Failed admission retains no partially accepted directory
and does not fall back to empty. `ready` still does not authenticate a peer;
TLS, discovery and Sessions remain inactive.

ADR 1453 defines the only typed control-identity payload accepted by the new
platform-store-to-TLS path: a bounded certificate chain, private key and
peer-specific CA with exact binary consumption. Decoded components are owned by
a drop-clearing Rust value, and Windows clears temporary encoded/decrypted
buffers. Android validates before creating a Keystore entry and after AEAD
decryption, preventing authenticated legacy arbitrary bytes from becoming TLS
configuration. These measures limit ordinary memory lifetime but do not promise
forensic erasure after allocator or TLS-library copies. The bundle has no peer
authority without the separately stored exact leaf fingerprint, active paired
record and successful TLS/Session binding. Issuance, rotation, recovery and
revocation remain absent.

ADR 1460 keeps Android's first catalog exchange behind the same authenticated
binding and exact peer identity checks as Windows. A catalog with a wrong
Session or Node cannot survive initial admission; any exchange failure closes
the transport and reconciles the provisional Session. This boundary receives
an authenticated channel and therefore does not itself prove secure listener
configuration or credential selection.

ADR 1461 makes Android listener trust selection unambiguous: exactly one active
paired record supplies the expected client Node, exact leaf fingerprint and the
handle whose typed credential was recovered. Zero or multiple active records
fail before a server starts. This single-peer restriction is a current security
boundary, not a claim that the wider directory format is single-peer.

ADR 1462 transfers the recovered typed credential into native listener
construction and overwrites the temporary Java array afterward. The listener
and Runtime registries are bounded, non-exported JNI state. Teardown closes the
authenticated channel and exact Session before Runtime removal. Binding to all
interfaces does not itself grant access: mutual TLS, leaf pinning and Node
binding remain mandatory before catalog admission.

ADR 1463 exposes only Node ID, ephemeral control port and a random restart nonce
in Android's discovery datagram. It carries no name, certificate, credential or
capability inventory. TTL one limits intended scope but is not an authorization
boundary; the receiving host must continue exact pairing and TLS checks.

ADR 1464 does not treat retained peer metadata as current authentication. Every
reconnect performs fresh mutual TLS, exact leaf pinning and a fresh bound
Session. A previous non-closed Session blocks admission to prevent concurrent
duplicate control ownership.

ADR 1465 heartbeat messages are accepted only on the already authenticated TLS
channel and must match its exact Session. Their sequence is liveness correlation,
not a nonce for enrollment, replay authorization or certificate rotation.

ADR 1466 assigns heartbeat initiation to the Windows dialer in the current
single-direction deployment. Android rejects unexpected acknowledgements,
echoes only a validated request sequence and reconciles the bound Session after
transport/protocol failure. Heartbeat health never grants a Port lease or starts
hardware.

ADR 1467 transmits only the Runtime-validated, Source-filtered publication
catalog after local selection changes. The receiver admits it through the
existing exact TLS/Session/peer binding and duplicate guard; changing a catalog
does not grant a lease, authorize a Route or start a device. A malformed,
wrong-identity or unexpected message closes the connection.

ADR 1469 adds a bounded offline first-pairing invitation. The binary package is
bound to one exact target Node UUID and rejected on malformed lengths, trailing
bytes or target mismatch. Windows stores its client identity with current-user
DPAPI before committing non-secret metadata; Android validates the package in
Rust, writes the server identity through Android Keystore, then atomically
commits its directory. Secret bytes never cross the WebView interface. The file
does contain an Android private key before import, so the alpha UI requires
private transfer and deletion after use. It is not a PAKE, expiry, rotation or
recovery protocol and must not be presented as production enrollment.

ADR 1470 permits a numeric endpoint fallback only after pairing. The supplied
IP address and port never select a credential or peer identity: the exact Node
record, DPAPI bundle, server name and pinned leaf fingerprint still come from
the active paired directory, and the TLS transcript remains bound to both Node
IDs and a fresh Session. Unpaired or revoked targets cannot use this path.

ADR 1476 allows the UI to cache only a Node ID and numeric endpoint after a
successful authenticated control connection. Discovery and cached endpoints
are routing hints, not trust records. Every reuse resolves current active
pairing metadata and re-runs the same pinned mTLS/Session checks; revocation
invalidates cached selection. No resource grant or data Route is inferred.

ADR 1477 makes Android's existing all-interface control listener prefer a
stable numeric port, falling back only on address-in-use. Port predictability
does not weaken the required pinned mTLS check, but the unpaired listener's
resource-pressure surface remains an alpha risk requiring a production review.

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

ADR 1478 lets a user explicitly retain an idle Android control listener via
the existing visible connected-device foreground Service. The setting is off
by default, restored only upon Activity launch after process death, and cleared
by Stop All. It does not grant peer trust or a hardware Route; incoming control
still requires the paired pinned mTLS identity and resource authorization.

ADR 1479's Android paired-peer UI projection contains only Node ID, display
name and revoked state. The Service never sends the canonical directory,
credential handles or pinned certificate fingerprints to the WebView. Seeing
a paired name is not proof of an authenticated connection or a capability
grant; the shared UI labels those states separately.

ADR 1480 gives **new** paired control leaves both TLS ClientAuth and
ServerAuth EKUs so either Node can eventually dial. It retains exact peer-leaf
pinning, dedicated CA, ordered Node binding and fresh Session IDs on every
connection; it does not bypass resource consent. Existing single-role leaves
remain valid for their present direction and cannot be used in reverse without
an explicit re-pair/rotation transaction.

ADR 1481's opt-in Windows inbound diagnostic binds only an explicitly named
local IP/port and one active paired Node. Unknown/revoked peers and invalid
certificate or Node binding fail before catalog admission. The 20-second
accept window and per-connection I/O timeout bound this diagnostic; it exposes
no unpaired enrollment endpoint and starts no hardware Route. It is not
automatically opened by the desktop app.

ADR 1482's Android outbound host accepts a numeric endpoint only as a routing
hint. Rust pairing validation, exact Windows leaf pinning and ordered Node
binding precede Session/catalog admission. Invalid or missing paired material
fails closed, and an unsuccessful catalog exchange reconciles the Session.
The host path does not silently grant a Route or expose credentials to WebView.

ADR 0043 requires a visible user action plus granted recording and notification
permission before the 001C microphone Source starts. The non-exported service
uses the microphone/media-playback foreground types, a persistent notification
with Stop action and `START_NOT_STICKY`; Activity closure is not implicit
authorization to restart capture.

ADR 1421 adds the normal `CHANGE_NETWORK_STATE` permission and
`FOREGROUND_SERVICE_CONNECTED_DEVICE` to the unified APK. They are used only by
the non-exported Node service while an explicitly user-started IMU, gamepad,
touchpad or haptics capability owns a network-device interaction lifecycle.
They do not authorize arbitrary peer discovery, change Wi-Fi configuration,
weaken Route admission, or bypass the existing camera/microphone consent rules.
On Android 13+, connected-device capability Start fails closed until the user
grants notification consent, preserving the visible-lifecycle policy.

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

ADR 1428 rejects ambiguous Android display properties before the receiver can
open: complete field sets, duplicate decoded keys, unknown fields and bounded
values are checked independently of TLS identity. Failed TLS socket setup now
closes the created socket before propagating failure; only successful setup
transfers ownership. CPD1 temporary caller-owned key bytes are wiped after
configuration copying, and GameStream binding validation precedes key loading.
Neither preflight nor these lifecycle fixes issue credentials or grant peer trust.

## Linux inherited-stdio host

ADR 1514 default mode exposes only local status/snapshot/shutdown on inherited
process pipes; ADR 1515 below adds an explicit mock-only mode.
It opens no listener, establishes no peer trust and creates no credentials or
hardware grants. The caller supplies a public non-nil Node ID, which is identity
metadata, not authentication. Strict request shape and byte bounds reject malformed
input. Stdio backpressure remains the local process owner's responsibility; this
is not a hardened multi-client service or a production network security claim.


## Linux explicit mock mode (ADR 1515)

The default ADR 1514 host creates no peers or grants. Explicit `--mock-routes`
launches only the two known mock executable names beside the node executable,
without shell/arguments and using embedded repository manifests. The directory
must contain trusted repository builds: Adapter ID validation is not executable
authentication. Synthetic peer/Session/consent is visibly test-only, never real
trust. Fixed mock lifecycle commands cannot name arbitrary paths, PIDs or peers.
`mock_source_crash` targets only the owned smoke-test source. No credentials,
leases, network, physical permission or production authorization is introduced.
See `adr/1515-linux-mock-route-lifecycle.md`.

ADR 1516 permits an explicit finite Unix-local mock IPC fixture, not IP networking.
The host creates and owns one private temporary directory/socket per epoch.
Repository children exchange one bounded token and return only receipt metadata.
No grants, credentials, services or system security settings change. Same-user
processes and caller-controlled temporary-directory ancestry remain trusted;
this is not peer authentication. Cleanup closes the pair before removing owned
paths. Forced termination can leave residue; no broad cleanup is authorized.

ADR 1517 retains the trusted local-fixture boundary for explicitly selected
four-record batches. Count, token, ordering and timestamps are fixed; records
remain at most 2048 bytes. Complete receipt validation precedes Active, and
partial/error paths retain child-before-path cleanup. No peer authentication,
IP networking, hardware access or asynchronous Stop guarantee is added.

ADR 1518 adds explicit finite Linux playback lab opt-in, private/loopback/overlay
IPv4 endpoints and an expected UDP peer. These are scope limits, not authentication:
static lab bindings remain spoofable and replayable across launches. Existing
Windows/Android policies are unchanged. No default Linux listener or device action
is enabled. PipeWire child IPC is bounded; no media enters control messages/logs.

ADR 1519 checks dynamic Linux audio Route grants and local Sink leases before
resource creation and each PCM delivery, including after receive. A Ready
Session is not authentication; the trusted caller still owns peer admission,
negotiation and local consent. The library is not a remote authorization API.
UDP remains insecure lab transport, with no automatic static-binding fallback.
Default Linux management opens no listener. Paused polling stops data forwarding
but resource expiry/closure is caller-driven, not an independent watchdog.

ADR 1520's accepted Linux owner requires the exact expected paired TLS
Session/local/peer binding plus existing finite local authority. Peer audio
offers are read from that channel; peer IP comes from its TCP socket. This does
not secure UDP media. Timeout/disconnect closes the dedicated Session and media
at the next driven control I/O; no independent watchdog is claimed. No new
credential, grant, listener or default device operation is created.

ADR 1521's explicit host supervisor continuously checks local Stop/owner loss and
finite authority while polling control/media fairly. Incremental application
frames have nonrenewable deadlines; encrypted-only traffic is work-bounded and
cannot bypass the host's grant/lease/lifetime checks. No listener or credentials
are added. This does not secure UDP or create consent from config/peer Ready.
Executable credential/admission/consent prerequisites are listed in the ADR.

ADR 1522 requires an explicitly invoked lab command and an exclusive trusted
FIFO launcher. Stdin inheritance is not authentication or confidentiality from
its invoker. Existing credential/paired-record handoff is one-shot, bounded and
nonpersistent; diagnostics never echo secret/parser content. Only fresh exact
local approval after real paired TLS/catalog creates finite grant/lease.
Transient clearing is best effort, not secure erasure of Rustls/process memory.
Actual credential access, deployment and device setup require separate approval;
the source milestone does not provide it. UDP remains unauthenticated lab media.

ADR 1533's offline Linux AVC boundary treats compressed data as untrusted:
bounded CAVC records, cumulative budgets, bounded SPS/PPS/slice inspection,
actual AVFrame validation, fixed owned subprocess, nonblocking private IPC,
absolute deadlines and kill/reap. Decode completion grants no authority;
`RuntimeCamera` rechecks the exact Session/Route/lease/epoch before projection.
No network socket listener or arbitrary executable/URL/config path is accepted.

ADR 1534 adds a distinct finite camera data TLS channel, never an upgrade-in-place
of legacy plain CAVC. Existing exact peer pins, TLS 1.3, separate ALPN and full
Session/Route/ports/Stream/epoch/expiry ACK precede helper launch. Dynamic endpoint
host is derived only from authenticated control, not payload metadata. Separate
resource consent and Source/Sink leases remain mandatory. Control disconnect,
Stop/revoke/expiry, malformed input and bounded-queue failure close the dedicated
session and reap decoder/projection. New source code does not authorize use or
creation of real persistent credentials; software tests use ephemeral identities.

## Linux touchpad device lifetime

ADR 1535 permits only an explicitly admitted exact touchpad Route/Session/Stream
and finite local Sink lease to create an owned Type-B device after matched pinned
TLS Hello. Fixed `/dev/uinput`, immutable descriptor, single-use factory, bounded
IPC and 240-report/s limit exclude peer-supplied paths and arbitrary injection.
No module, udev, permission or privilege changes occur. Missing access fails closed.
Stop/revoke/expiry/EOF faults the epoch; ACK requires a committed full SYN_REPORT
and fresh admission. Normal cleanup releases slots/keys and destroys the device;
hard process kill can only guarantee descriptor closure, not release events.


## Linux camera executable boundary

ADR 1536 reuses private inherited framing and strict paired-directory/credential decoding. Exact request/Route approval, actual published Source validation, independent finite peer consent, both leases and data mTLS precede fixed sibling helpers. No password/helper path input, enrollment, auto-renew, physical camera permission or daemon/security change is added. Local Stop/EOF and all owner failures are bounded and dispose of owned resources.

ADR 1537's finite Linux touchpad launcher never treats a paired control Session
as input consent. Exact local nonce/Route approval, independent Android grant,
Source lease and local Sink lease all precede pinned touchpad data/Hello and
native open. Offered hosts are never accepted: the data IP is derived from the
authenticated control socket. The existing independent worker watchdog also
observes the shorter Source deadline through the shortened local Sink lease.
Fixed helper siblings and `/dev/uinput` require existing permissions; no
permission change, arbitrary executable or device path is accepted. Test-only
fake syscalls use a separately named feature/command/helper and do not alter the
production command. An interrupted cleanup remains an error, not a release claim.

## Linux audio helper stream overrides

ADR 1538 removes six upstream PipeWire stream override environment variables in
the shared fixed helper builder for every audio endpoint and the capture probe.
Virtual-speaker identity is checked against the admitted local Source before
opening the no-autoconnect Audio/Sink. Private remote/config routing remains
trusted and inherited: the scrub is not a general configuration or process
sandbox. It changes neither default devices nor user/session settings, and does
not make existing trusted-lab UDP confidential or authenticated.

## Experimental Linux pairing-store boundary

ADR 1539 adds bounded owner-bound non-secret metadata and recovery with an injected
protected-credential backend. ADR 1540 adds optional local Secret Service storage:
plain sessions do not encrypt local D-Bus traffic, and OS keyring acknowledgement
is trusted, not a power-loss/fsync guarantee. Cooperative namespace serialization
and duplicate checks are not attribute CAS against noncooperative writers.
Content-derived handles, exact bundle digests, exclusive
store ownership and revoked tombstones fail closed. Recovery never generates or
deletes credentials and does not prove remote completion. The typed selected-peer
bootstrap material changes no finite launcher, consent or lease checks.

ADR 1541's launcher integration reads one exact existing paired peer in-process;
selector/config identity mismatch and unavailable/revoked/altered records fail
before control-listener binding or helper spawn. It never recovers a pending
journal or grants permanent resource permission. The paired-store marker is not
approval. Existing three-frame private bootstrap remains a distinct default mode.

ADR 1542 requires stored peer-pin failure, revoked pre-launch state rejection,
and no helper/lease without fresh approval in actual executable fixtures.
Runtime Source revocation and finite expiry remain independent of stored pairing;
these tests do not turn per-launch metadata loading into a live revocation watch.

ADR 1543 applies this exact-peer snapshot to audio playback and capture, including
existing virtual endpoints. Audio UDP remains plaintext and trusted-lab only;
pinned control identity does not encrypt media or authorize a local resource.

ADR 1544 treats each join/revoke as a fresh explicit trust decision. PINs enter
only through private FIFO input; no manager error/PIN/credential status is echoed.
Existing and revoked peers cannot be replaced by first enrollment. Local saved
state is distinguished from bilateral completion, without automatic retry or
rollback claims; pairing never grants a resource or renews a lease.

ADR 1545 binds explicit recovery to a fresh request, exact peer and domain/owner-
bound journal digest. The store lock spans confirmation and local replay; revoked,
changed or missing records fail closed. No protected write/delete or remote trust
completion is inferred. Same-UID malicious filesystem races are outside flock
serialization; the staged feature inventory remains unsigned, not authorization.

ADR 1546 keeps welcome PIN output exclusively in an explicitly designated
owner-only FIFO distinct from standard streams. Concrete bind selection and
manual-only discovery policy prevent implicit exposure. Bilateral approval,
one-attempt lifetime and existing finite resource authority remain separate;
revoking invitation validity cannot erase already-delivered caller bytes.


ADR 1547 defines the opt-in Linux terminal pairing controller: a private-TTY
client of the existing finite Node management protocol, with exact fresh consent,
owned invitation FIFO cleanup and mandatory isolated two-controller PTY acceptance.
The five-binary staging inventory includes the console; only the paired profile
enables both console and Node pairing operations. No resource grants or renewal
are introduced.


ADR 1548 adds only interactive finite playback/virtual-microphone control to the
Linux console, with exact current-session confirmation and unchanged Node and
counterpart resource authority. Its mandatory isolated PTY/PipeWire profile is
separate from legacy/paired launcher tests. UDP audio remains unauthenticated
plaintext trusted-lab media; reverse audio is added separately by ADR 1549,
while camera and touchpad are added separately by ADRs 1550–1551; renewal
remains unimplemented.


ADR 1549 adds interactive finite selected-source stereo capture and virtual-speaker
control through the same console/Node boundary. Exact local Source → remote Sink
consent, independent counterpart authority and finite lease cleanup remain
mandatory. Sixteen additional actual-PTY/private-PipeWire profiles reuse the
independent stereo media oracle; all prior 48 graph profiles remain required.
No physical-device or secure UDP media claim is made.


ADR 1550 adds finite remote-camera-to-local-virtual-camera console control with
exact fresh session consent and strictly `Starting`, unverified-consumer status.
Thirteen mandatory actual-PTY camera profiles retain independent pixels, owned
process/graph cleanup and all existing media gates. No physical camera or
unattended lifecycle is introduced.


ADR 1551 adds finite touchpad console consent using the existing state-only
Node schema. Binding never implies activation; independent software-helper
acceptance retains full commit-before-ACK and contact-release proof. The fixture
action is compile-time-only and excluded from both staging profiles. No real
uinput device, permission change, OS gesture proof or renewal is introduced.


ADR 1552 adds a bounded paired-store authenticated catalog snapshot and terminal
inspection, without creating Routes, grants, leases or media endpoints. Empty
local publication and exact peer/certificate binding precede a bounded escaped
projection. Catalog-only hints never imply resource readiness or approval.


ADR 1553 adds camera-only manifest preparation from a current authenticated
catalog, with separate fresh exact selection and output confirmation. Exclusive
owned-file export never starts media or grants authority; later launch still
requires current catalog validation and independent finite session consent.

### Audio and touchpad manifest preparation

[ADR 1554](adr/1554-linux-audio-touchpad-manifest-export.md) extends the same
explicit snapshot/select/save path to four audio endpoint modes and touchpad.
Export remains nonsecret configuration only, with no Start or resource authority.

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

### Finite secure mono audio

[ADR 1559](adr/1559-finite-secure-mono-audio.md) adds a distinct pinned TLS 1.3
mono audio path, never a fallback flag on trusted-lab UDP. The exact selected
format, paired leaves, Session/Route/Source/Sink/Stream/epoch, original grant and
leases are immutable. A consumed offer, malformed/replayed record, generation
change, stall or expiry cannot restart the connection. Both ends enforce bounded
packet admission and monotonic pacing independently. Encryption does not confer
resource consent or prove audibility. The legacy Java DTO explicitly rejects
secure/unknown transport before opening UDP; physical capture and production
console secure-audio integration are outside this API-only slice. Traffic timing
and endpoint metadata remain observable, and TLS/TCP head-of-line blocking is
not a low-latency guarantee.

[ADR 1560](adr/1560-android-secure-microphone-binding.md) adds an explicit Android
secure microphone branch, not an exception to the legacy DTO rejection. Its
closed schema never exposes credentials or a caller-chosen data endpoint to
Java. Only a native one-shot generation can admit fixed PCM frames after the
authenticated binding ACK. Stop is latched before registration, interrupts TLS
and writes once their socket exists, and cannot be undone by a late completion.
The preceding TCP connect retains its original bounded deadline. Existing
permission and visible lifecycle checks remain required; actual capture format
and routed-device identity must be checked before publication and during use.
Queue exhaustion and capture/transport stalls terminate without UDP fallback.
This does not claim physical-device compatibility or a production Linux secure
audio launcher, and does not introduce any new permission or renewable grant.


[ADR 1561](adr/1561-linux-secure-playback-launcher.md) adds explicit paired-store
Linux playback selection without weakening those boundaries. Secure mode requires
its exact profile, explicit local target and original finite consent; the old
private-credential launcher cannot select it. Catalog export is not authority
and cannot downgrade a secure-only Source. A monotonic consent deadline survives
control/TLS negotiation and is intersected with original grant/lease limits.
Secure status remains Starting: received/written PCM is not verified audibility.
[ADR 1562](adr/1562-linux-secure-virtual-microphone.md) permits an owned virtual
microphone instead of a selected target, with no target override and an exact
local Sink-derived identity. No system default or application consumption is
claimed. Legacy UDP and reverse stereo retain their separate limits.
No unattended service, new permission, renewable grant or physical-device claim
is introduced. Plain PCM remains at authorized local audio boundaries.

[ADR 1563](adr/1563-explicit-five-minute-secure-mono-profile.md) permits a distinct
explicit secure mono profile with requested duration at most 300 seconds. It
cannot be inferred from a larger grant or relabeled old profile. Exact
authenticated catalog/Route/profile and original full lease claims remain pinned;
every deadline intersects original local consent, grants and leases. The existing
global 300-second lease TTL stays unchanged. No rate, burst, queue, per-operation
stall, renewal or fallback allowance grows. Android live capture rejects this
new profile; only the production Rust Source and Linux path are integrated here.

ADR 1564's Android five-minute capture preference is capability selection, not
authority. Profile changes retire the prior pending/active generation before
catalog publication. Permission-return callbacks and actual recorder callbacks
cannot revive the previous profile; cleanup quarantine and the original finite
consent/lease cap remain mandatory. A mismatch never falls back to UDP.

ADR 1565's reverse secure stereo APIs require fresh local consent, authenticated
Start and exact original claims at both Ports before media setup. Current peer
identity is bound independently of control client/server role; mono, stereo and
legacy transports cannot substitute for each other. Fragment validation is
atomic, queues are bounded, and no reconnect or renewal follows failure. Android
JNI/AudioTrack advertisement and production launcher integration remain separate
acceptance boundaries; sink acceptance is not proof of physical audibility.
