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

## Milestones

1. Foundation: deterministic validation and process boundaries.
2. Local real-Adapter lab: clearly marked insecure/local mode.
3. Pairing spike: device identity and authenticated transcript.
4. Production transport: encryption, replay and downgrade binding.
5. Projection/driver hardening: ACLs, isolated tests, fuzzing and review.
6. Release: signing, SBOM, vulnerability and update process.

No build before milestone 4 is described as safe for untrusted networks.
