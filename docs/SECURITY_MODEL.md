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
