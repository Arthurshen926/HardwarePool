# CapyIO Windows Camera Media Foundation Boundary

This crate contains the Windows-only, in-process COM projection used by
CAPY-CAMERA-001B1B. It exposes one `IMFMediaSourceEx` and one `IMFMediaStream2`
for the deterministic 1280x720 30 fps NV12 fixture.

The source strongly owns its stream while the stream keeps a weak source
reference. Source and stream use separate thread-safe Media Foundation event
queues; shared Rust state is bounded and synchronized. The source advertises
`IMFSampleAllocatorControl`, initializes the Frame Server-provided sample pool
and includes the mandatory Legacy sensor profile. `RequestSample` never waits
for a contended lock, preserves an optional caller token and publishes one
positive-stride 2D NV12 sample with QPC-correlated 100 ns timing.

The 001B1B integration test activates the objects directly inside the test
process after `MFStartup`, then shuts them down. It does not use the class
factory or registration backend and cannot make a camera visible to other
applications.

CAPY-CAMERA-001B1C adds a standard class factory/DLL export surface whose
factory creates `IMFActivate`, mandatory Frame Server `IMFGetService`,
`IKsControl` and allocator behavior, and a real but closed session/current-user
registration backend. Normal tests still construct only in-process objects and
never write the registry or call `IMFVirtualCamera::Start`.

The `capyio-camera-virtual-lab` binary accepts only `preflight`, `roundtrip`,
`shared-roundtrip`, `gui-hold`, `live-hold`, `trusted-lan-live-hold`,
`consumer-probe` or `cleanup`. Only `trusted-lan-live-hold` accepts parameters:
exactly one Windows bind IPv4 and one phone IPv4. It has no caller-controlled
path, CLSID, access, lifetime, camera name, child count, port or timeout.
`preflight` and `consumer-probe` are non-registering.
`roundtrip`, `shared-roundtrip` and `cleanup` are controlled system-state
operations and require the exact host/package/rollback approval recorded in the
lab report. Shared-roundtrip keeps registration in one parent while two
independent fixed-argument child processes run sequentially. The parent passes
its exact virtual-camera symbolic link through a bounded internal environment
value; each child independently enumerates, activates and reads two frames. A
20-second deadline applies to each child, and an unfinished child is
killed/reaped before registrar cleanup. This validates sequential process-level
reuse only; simultaneous multi-consumer fan-out is not established.

CAPY-CAMERA-001C14 adds the closed `live-hold` orchestration command. It refuses
an existing production mapping or CapyIO Camera registration, spawns only the
fixed sibling `capyio-avc-lab-receiver.exe` with fixed arguments
`--max-access-units 7200 --publish-shared`, and waits at most 30 seconds for a
validated Global mapping. Only then does it expose the existing
Session/CurrentUser camera for the fixed 180-second hold. During the hold it
checks both receiver liveness and mapping validity every 100 ms. Every outcome
stops/reaps the receiver, runs registrar Stop/Shutdown and requires the mapping
to disappear within five seconds. It does not deploy the COM DLL, install an
APK, establish ADB reverse, start Android capture, elevate itself or accept any
path/port/duration. Running it remains a separately approved system-state lab
operation.

CAPY-CAMERA-001C23 adds
`trusted-lan-live-hold <windows-ipv4> <phone-ipv4>`. It passes those two literals
to the fixed sibling receiver as `--trusted-lan-bind` and
`--trusted-lan-peer`; receiver validation restricts them to different
RFC1918/link-local/100.64.0.0/10 addresses and fixed port 38173. Registration,
mapping readiness, 180-second hold, child liveness and cleanup remain identical
to `live-hold`. The command performs no discovery, DNS lookup, firewall change,
ADB operation or Android launch. It is plaintext trusted-lab behavior and
remains a separately approved system-state operation.

CAPY-CAMERA-001C24 retained the closed command surface and fixed 60-second hold,
but allows up to 120 seconds for the first validated Global mapping and gives
the receiver a fixed 60-second replacement-stream grace. Finalization always
attempts receiver and mapping cleanup; either cleanup failure is reported as
`stage=cleanup` and is no longer hidden by an earlier liveness/validation error.
These are bounded orchestration changes only. Deployment, registration and a
physical run still require exact package/host/rollback approval.

C28 extends the fixed GUI/live hold to 180 seconds so ordinary Windows Camera
can cover placeholder, live, producer loss and recovery in one bounded run.
The administrator deployment script also adds the explicit
`RemoveWithFrameServerRestart` cleanup action. That action stops only the
Windows `FrameServer` service when it was running, removes the fixed lab CLSID
and DLL, and restarts/waits for the same service in `finally`. Plain `Remove`
remains non-disruptive. Either system-state action still requires the exact
host/package/rollback approval; the script never selects the disruptive path
implicitly.

CAPY-CAMERA-001C15 adds the non-elevating, read-only
`scripts/capyio-camera-live-lab-preflight.ps1`. It has no parameters and hashes
the exact receiver, orchestration executable and COM DLL, verifies the deploy
script pins that DLL, then rejects an existing CapyIO ProgramData root, fixed
CLSID, TCP 38173 listener or camera-lab process. It does not inspect or change
ADB state because an exact current device target belongs to the separately
authorized physical run. Passing this preflight is package/host cleanliness
evidence only; it does not create or prove a virtual camera.

CAPY-CAMERA-001B1E adds a non-registered constructor backed by a shared
process-local `ExternalNv12FrameIngress`. Its `RequestSample` path uses only
non-blocking ingress/runtime locks, returns `MF_E_NOTACCEPTING` when no decoded
frame is ready and carries queue gaps into
`MFSampleExtension_Discontinuity`. Tests prove caller-owned payload bytes reach
the provided allocator. The class factory and system lab remain fixture-backed;
there is no cross-process shared memory, network input or Android capture yet.

CAPY-CAMERA-001B1F added the first explicit decoded-frame process boundary. One
producer creates `Global\\CapyIO.CameraIngress.v1`; independent consumers open
read-only views. The ABI is versioned and fixed at 4,147,648 bytes: a 256-byte
header plus three 64-byte slot headers and three 1,382,400-byte packed NV12
payloads. Stream ID, positive epoch, generation, 720p30/NV12 metadata and every
publication are checked. Per-slot commit markers let a consumer reject a frame
that is being replaced, and a skipped publication becomes a discontinuity.
The mapping DACL grants SYSTEM, administrators and the owner full access while
LocalService receives read-only access. A non-registered constructor can
project this consumer through the existing MF sample path. The registered COM
class factory is deliberately still fixture-backed; Runtime production,
Android capture, transport/decode and a system external-frame roundtrip remain
outside this gate.

CAPY-CAMERA-001B1G adds a composed process test rather than another production
surface. Its parent publishes a frame into a unique local test mapping; a
separately spawned test process opens the read-only consumer, starts Media
Foundation, supplies the platform allocator and verifies the exact shared luma
byte and sample duration. A subsequent request returns
`MF_E_NOTACCEPTING`. This proves the shared payload reaches a real MF sample
across a process boundary, but still does not exercise the registered class
factory or the Frame Server service identity.

CAPY-CAMERA-001B1H moves the mapping implementation to sibling crate
`capyio-windows-camera-share`. This COM crate now imports only the consumer
contract and no longer creates producer mappings or owns raw file-mapping
security calls. Cross-process MF tests use the share crate's bounded,
feature-gated local test namespace. At that gate, production registered
activation was still fixture-backed.

CAPY-CAMERA-001C6 changes only the registered provider selection. Activation
opens the fixed Global mapping when its complete protected header is valid. It
falls back to the deterministic fixture only when `OpenFileMappingW` reports
that the object is absent; access denial, malformed headers and every other
error fail activation. Direct in-process constructors remain deterministic and
do not depend on ambient mapping state. The fixed Local integration-lab mapping
is deliberately invisible to registered activation.

CAPY-CAMERA-001C7 makes only the registered shared provider asynchronous. A
fixed four-request reservation bound feeds a Media Foundation serial work queue;
when the 30 fps mapping has no newer frame, a scheduled callback retries after
5 ms instead of returning the pipeline-fatal `MF_E_NOTACCEPTING`. Request tokens
remain FIFO and stop/shutdown releases pending reservations. A regression test
publishes a later frame after an empty interval and observes its exact luma in
the retained request. The controlled V2419A Global/registered roundtrip then
enumerated one `CapyIO Camera` and read advancing NV12 samples through Frame
Server before exact deployment and registration rollback.

CAPY-CAMERA-001C26 makes mapping absence at registered activation recoverable.
Instead of permanently selecting the fixture, the source emits it as a
deterministic offline placeholder and checks only the fixed production mapping
after a fixed 15-placeholder-frame countdown on the existing serial work queue.
The first validated shared payload is rebased onto the active virtual-camera
identity/epoch/sequence/timeline and marked discontinuous. Live mode retains
pending requests while awaiting newer publications and never interleaves
placeholder frames. Invalid/access-denied mappings still fail closed. The
feature-gated Local mapping target exists only for tests; no new command,
registration, service, transport or permission is added.

CAPY-CAMERA-001C27 makes that registered provider bidirectional. Both an
initially present and initially absent production mapping use the asynchronous
lifecycle provider. After 400 consecutive empty polls on the existing 5 ms
sample pump, it releases its consumer handle and resumes the placeholder at the
next virtual sequence/timestamp with a discontinuity. The fixed 15-placeholder-
frame probe interval remains in force. Producer generation/source identity and
pre-rebase sequence/timestamp suppress stale replay when a paused mapping is
reopened; a newer publication or replacement mapping generation reattaches and
is marked discontinuous. The counter advances only while a sample request is
pending, so it is a bounded demand-driven fallback rather than a producer
heartbeat.

CAPY-CAMERA-001C8 adds the parameter-free `gui-hold` lab command. It verifies
one exact camera enumeration match, holds the same Session/CurrentUser camera
for 180 seconds, and then runs the existing Stop/Shutdown cleanup. This bounded
window allowed a fresh Windows inbox Camera launch to display live V2419A
pixels through `CapyIO Camera`; the screenshot was not retained. The command
does not accept caller-selected paths, CLSIDs, scopes or durations.

CAPY-CAMERA-001C9 revalidated the same bounded GUI path after Android queue and
codec-latency hardening plus verified Windows H.264 decoder low-latency mode.
Windows inbox Camera again displayed continuous live V2419A pixels. A prepared
local millisecond clock was not in the phone's field of view, so this run adds
no numeric end-to-end latency claim. Exact registration/deployment cleanup
passed.
