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
