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

The VIIPER Xbox 360 boundary accepts only an explicit IP-literal loopback socket
with a non-zero port, positive bounded connect/I/O deadlines and a response
limit no greater than 4 KiB. It has no default address, DNS, discovery,
authentication-key lookup or generic request API. Independent read-only probe
sends exact `ping\0`, reads JSON through connection close under one absolute
I/O deadline, uses fixed DTOs and accepts only exact server/version identity.

ADR 0042 governs resource creation. The mutating entry point requires a caller
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
requires the ADR 0042 caller assertion before installing a Route controller,
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
any required restart and attachment authorization are complete; like ADR 0042,
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

### Kernel compromise

No network, DNS, pairing, crypto negotiation, JSON/Protobuf, codecs or reconnect
logic in drivers. Kernel IPC is fixed/bounded and validated. Driver tests
default to an isolated approved target. ADR 0029 permits bounded Gate 7B
install/enumeration/playback/uninstall checks on the identified local lab after
recovery and rollback preflight. Driver Verifier and boot/security-policy
changes remain separately approved high-risk actions.

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
