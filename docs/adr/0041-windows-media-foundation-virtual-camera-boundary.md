# ADR 0041: Bound the Windows virtual camera to user-mode Media Foundation

- Status: Accepted
- Date: 2026-08-29

## Context

The deterministic CAPY-CAMERA-001A source produces the canonical packed NV12
1280x720 30 fps Video Profile, but ordinary Windows applications need a system
Projection. Windows 11 exposes a user-mode Media Foundation virtual-camera API
for `SoftwareCameraSource`. Creating the object alone does not register a
camera; successful `IMFVirtualCamera::Start` creates/registers the underlying
camera and exposes its symbolic link. All-user access requires administration,
system lifetime persists beyond the creating process, and privacy policy can
still deny access.

The custom media source is loaded by Windows Frame Server in a low-privilege
context. Samples require QPC-correlated 100 ns timestamps. The official custom
source contract uses `IMFSampleAllocatorControl` and the Frame Server-provided
video allocator; the source must also expose a Legacy sensor profile. These
constraints belong in the Windows Adapter, not `capyio-video`, Core, a driver
or the JSON-RPC control path.

## Decision

Use the Windows 11 user-mode Media Foundation `SoftwareCameraSource` path. The
initial Projection exposes exactly the CAPY-CAMERA-001A NV12 1280x720 30 fps
stream. There is no resize, rotation, codec negotiation or encoded transport in
this slice.

Split implementation into nine gates:

1. CAPY-CAMERA-001B0 is non-mutating. It implements a pure projection seam,
   checked packed-to-positive-stride NV12 copy, QPC-correlated timing mapper,
   closed lifecycle model and an opt-in probe that calls only
   `MFIsVirtualCameraTypeSupported`.
2. CAPY-CAMERA-001B1A fixes the allocation-free one-stream source/stream event
   protocol and registrar rollback orchestration against an in-memory backend.
   It contains no COM object or Windows registration backend.
3. CAPY-CAMERA-001B1B builds the windows-rs COM media source/stream and tests it
   through a local, non-registered activation harness.
4. CAPY-CAMERA-001B1C adds the session/current-user `IMFVirtualCamera` backend.
   Running `MFCreateVirtualCamera` followed by `Start`, or any stop/removal
   operation against the system catalog, requires a separately approved exact
   command and recorded rollback evidence.
5. CAPY-CAMERA-001B1D adds a bounded process-level sharing check. The registrar
   stays in one parent while two independently spawned consumers run
   sequentially and each enumerates and activates the public device. It adds no
   production IPC or transport and does not prove simultaneous fan-out.
6. CAPY-CAMERA-001B1E adds a process-local decoded-frame ingress and a
   non-registered projection test. It deliberately does not make a global
   mapping name, wire layout or producer identity part of the COM DLL contract.
7. CAPY-CAMERA-001B1F adds a versioned, fixed-size decoded-frame mapping and a
   read-only Frame Server-side consumer. It keeps the registered class factory
   fixture-backed until Runtime ownership and a controlled system external-
   frame roundtrip are separately reviewed.
8. CAPY-CAMERA-001B1G composes the mapping and non-registered MF source across
   two processes. The child verifies the platform sample rather than reading
   only the mapped bytes. It makes no registration or service-token claim.
9. CAPY-CAMERA-001B1H extracts the mapping ABI from the COM crate and adds an
   explicit headless producer lifecycle owner. It still adds no transport,
   decoder, Android capture or registered-provider switch.

The 001B1C media-source DLL implements the mandatory Frame Server
`IMFGetService`, `IKsControl` and `IMFSampleAllocatorControl` interfaces in
addition to `IMFMediaSourceEx`. It exposes no optional services and no camera
controls, returning the platform-prescribed unsupported results. Its standard
`IClassFactory` rejects aggregation, creates an `IMFActivate` object only for
the fixed CapyIO source CLSID and tracks active COM objects plus server locks
for `DllCanUnloadNow`. Activation copies the supplied attributes into the
source, which adds the mandatory Legacy sensor profile.

The real backend maps only the closed plan to
`SoftwareCameraSource`/session/current-user and only
`KSCATEGORY_VIDEO_CAMERA`. `MFCreateVirtualCamera` is the prepare step;
successful `Start` must yield a bounded symbolic link before the registrar can
report success. Stop and terminal shutdown remain separate operations. For
session lifetime, shutdown is the removal boundary. A best-effort Drop guard is
only a final crash-path safety net and never replaces explicit registrar
cleanup.

The controlled lab executable accepts `preflight`, `roundtrip`,
`shared-roundtrip`, `gui-hold`, `live-hold`, `consumer-probe` or `cleanup`, with no additional
arguments. Roundtrip requires one exact Media Foundation enumeration match and
pulls two NV12 samples through a Source Reader created from
`IMFVirtualCamera::GetMediaSource`. It bounds empty live-source reads, requires
the full frame size, exact 30 fps duration and monotonic bounded downstream
timestamps, copies opaque output into bounded CPU memory for luma validation,
then stops and shuts down through the registrar. Deployment is restricted to
one hash-recorded DLL and one CLSID key; it is not part of normal tests or CI.
Shared-roundtrip keeps that registrar alive while directly spawning exactly two
copies of the current executable, one after the other, with the fixed
consumer-probe argument. The parent supplies the exact symbolic link from the
started camera through a bounded internal environment value. Each child
independently enumerates an exact match, activates through `IMFActivate` and
validates two frames. The parent uses a fixed 20-second deadline per child and
kills/reaps an unfinished child before cleanup. A repeated `Start` while the
source is active retains the generation, outstanding requests, allocator and
sample timeline and publishes updated/start events, following the platform
sample's shared-source behavior.

The later fixed `live-hold` command composes the existing receiver and
Session/CurrentUser registrar without widening either contract. It accepts no
arguments, locates only a sibling `capyio-avc-lab-receiver.exe`, supplies the
fixed `--max-access-units 7200 --publish-shared` arguments, waits at most 30
seconds for the validated Global mapping and holds registration for the
existing 60-second GUI window. Existing mapping or registration state is a
preflight failure. Receiver exit or mapping loss fails the hold, and all paths
stop/reap the child, run registrar Stop/Shutdown and require mapping removal.
The command deliberately does not deploy the COM DLL, configure ADB, install or
start Android software, elevate itself, or form a production lifecycle API.

The first external ingress is fixed to the canonical 1280x720 30 fps packed
NV12 stream. It owns each accepted payload, binds one stream ID and epoch,
requires advancing sequence and source timestamp and uses the existing maximum
twelve-frame bound with drop-oldest behavior. Media Foundation consumes it only
through a non-registered constructor. An empty or contended ingress fails fast
with `MF_E_NOTACCEPTING`; a dropped-frame gap sets
`MFSampleExtension_Discontinuity`. COM class-factory activation continues to
construct the deterministic fixture provider; B1F defines the first local
process boundary below without switching registered activation.

CAPY-CAMERA-001B1F fixes that first process contract at
`Global\\CapyIO.CameraIngress.v1`. Exactly one producer creates the mapping; a
second creator fails closed on `ERROR_ALREADY_EXISTS`. Consumers request only
`FILE_MAP_READ`. The protected DACL is
`D:P(A;;GA;;;SY)(A;;GR;;;LS)(A;;GA;;;BA)(A;;GA;;;OW)`: SYSTEM,
administrators and the owner have full access, while LocalService can only
read. The mapping is ephemeral and Windows destroys it after the final handle
closes. A malicious administrator remains a privileged actor; the generation
is freshness evidence and not an authentication secret.

The v1 ABI is exactly 4,147,648 bytes: one aligned 256-byte header followed by
three aligned slots. Each slot contains a 64-byte header and a 1,382,400-byte
packed NV12 payload. The immutable header binds magic/version/layout, canonical
1280x720 30 fps NV12/BT.709 metadata, producer PID, typed stream ID, positive
epoch and non-zero generation. Publication zero means empty. The producer
invalidates the selected slot, copies validated owned metadata/payload, commits
the slot, then publishes the monotonically increasing publication number. A
consumer loads the latest number, verifies the slot commit before and after its
bounded owned copy, validates the reconstructed frame and advances only its own
cursor. A replaced slot yields no frame; a skipped publication is marked
discontinuous. This is a latest-frame contract, not a lossless queue.

The raw mapped view is isolated in `capyio-windows-camera-share`. Handles and
views have no Windows thread affinity and are moved, never shared directly;
mutable producer/consumer state requires exclusive access. Release/acquire
atomics and the post-copy commit check bound publication visibility across
processes. The MF request path returns `MF_E_NOTACCEPTING` when no new stable
publication is available and does not wait. The B1F constructor is
non-registering and accepts an already validated consumer. B1H adds a producer
host that owns start/publish/stop and releases on Drop, but no listener or
service executable. Registered activation, Runtime Route ownership and remote-
frame authorization remain deliberately unchanged.

The first registrar plan can represent only `MFVirtualCameraLifetime_Session`
and `MFVirtualCameraAccess_CurrentUser`. System lifetime and all-user access are
deliberately not representable. The background Runtime/Service owns lifecycle;
the desktop UI remains a client. A successful future start must retain the
returned symbolic link, while stop/shutdown and process-exit cleanup provide
bounded rollback. Session shutdown is the default removal boundary.

At projection start, bind the CapyIO stream ID/epoch and source timestamp to a
QPC-correlated 100 ns anchor. Derive every sample time from the absolute source
delta rather than accumulated frame duration. Require advancing sequence and
sample time. Copy each validated packed NV12 row into a positive-stride Media
Foundation 2D buffer, clear padding, and reject short pitch, short destination
or arithmetic overflow. Continuous frame bytes never use stdout or JSON-RPC.

The protocol core mirrors the official single-stream ordering: first start
queues new-stream, stream-started and source-started actions; restart substitutes
updated-stream. A fixed FIFO admits at most four sample requests, completes them
in order, cancels a ticket transactionally when sample construction fails and
cancels all outstanding requests on stop/shutdown. The registrar orchestrator
attempts shutdown when start fails, records cleanup-required state when both the
operation and rollback fail, and permits explicit cleanup retry.

CAPY-CAMERA-001B1B places unsafe Windows calls in the dedicated
`capyio-windows-camera-mf` crate. Its source strongly owns the stream and the
stream holds only a weak source reference, preventing a COM reference cycle.
Source and stream have separate Media Foundation event queues. Their shared
Rust state contains only mutexes, atomics and deterministic source state; the
Media Foundation interface wrappers remain owned by the corresponding COM
objects. The source initializes a ten-sample Frame Server-provided allocator
pool for the selected media type. `RequestSample` uses non-blocking `try_lock`,
allocates from that pool, writes through `IMF2DBuffer2::Lock2DSize`, sets the
bounded current length, preserves the optional request token and queues exactly
one `MEMediaSample`. A transient sample-construction failure cancels only its
accepted FIFO ticket, queues `MEError` and leaves the started stream
recoverable.

Use target-specific `windows` 0.61.3 and `windows-core` 0.61.2 dependencies with
only the recorded Win32 feature families. B1F additionally uses the already
workspace-resolved target-specific `windows-sys` 0.61.2 package in the dedicated
camera-share crate for raw file-mapping, security-descriptor and process APIs.
These packages are maintained by
Microsoft, licensed MIT OR Apache-2.0 and locked by Cargo.lock. `windows-sys`
matches the repository's existing bounded audio-mapping boundary and avoids
hand-written Win32 declarations; using the higher-level `windows` crate for raw
shared-memory byte layout would add wrapper conversions without reducing this
unsafe boundary. The official Windows-Camera sample is design evidence only;
no sample source or binary is imported.

## Alternatives

- a kernel camera driver: rejected because Windows provides the required
  user-mode path and drivers would add signing, deployment and kernel risk;
- DirectShow-only virtual-camera code: rejected for the first Windows 11 path;
- all-user/system registration: rejected because it expands privilege,
  persistence and rollback scope;
- hand-written FFI or imported C++ sample sources: rejected in favor of locked
  generated Rust bindings and a CapyIO-owned minimal media source;
- custom sample buffers or JSON-RPC frame payloads: rejected because platform
  guidance and CapyIO data-plane invariants require bounded MF buffers.
- named pipes or sidecar stdout for decoded frames: rejected because a 720p30
  stream is a high-bandwidth data plane and needs a bounded latest-frame path;
- a mutex-protected lossless shared queue: rejected because a stalled Frame
  Server consumer must not block the producer or grow retained video memory.

## Consequences

The projection contract, timing and buffer layout can be tested without a
camera or system mutation. The 001B1B integration test initializes COM and
Media Foundation, constructs the source/stream directly inside the test
process, validates event/sample behavior, then shuts both down. A positive
support probe and passing in-process test establish only API/type and local COM
behavior; they do not prove registration, enumeration, Frame Server activation,
privacy access, ordinary-application compatibility or catalog cleanup.

CAPY-CAMERA-001B1C completed the approved DLL/COM deployment, Frame Server
activation, controlled enumeration/two-frame pull and exact cleanup on
`DESKTOP-AT8EVE9`. The host required an administrator token for `Start` even
though all camera privacy toggles were already enabled. The final rollback
removed the session registration, fixed CLSID, DLL and empty lab directories.
Ordinary-application evidence and a non-elevated product workflow remain later
controlled work.

CAPY-CAMERA-001B1D narrows the first of those gaps to independent process-level
Media Foundation consumers executed sequentially. A simultaneous two-consumer
host probe still failed with `MF_E_HW_MFT_FAILED_START_STREAMING`, even after
the repeated-start state transition was corrected. The gate therefore does not
claim concurrent multi-application fan-out, compatibility with named
third-party camera applications, physical-camera capture, remote-frame ingress
or a non-elevated production lifecycle.

CAPY-CAMERA-001B1E proves that already-decoded caller-owned payloads can replace
fixture generation inside the MF projection without weakening bounds or
callback behavior. It does not prove Frame Server cross-process delivery of
those external payloads; the controlled system regression remains fixture-
backed.

CAPY-CAMERA-001B1F proves the fixed mapping ABI, duplicate-producer rejection,
independent latest-frame readers, discontinuity on skipped publication and an
actual child-process read-only open/payload copy. It also connects the consumer
to the non-registered MF provider. It does not prove that the Runtime produces
the mapping, that the registered Frame Server class factory selects it, that
LocalService access works on the target host, or that Android/network frames
reach an ordinary application. Those are explicit later gates.

CAPY-CAMERA-001B1G closes the remaining non-registering composition gap: the
producer remains in a parent process and a separately spawned child opens the
read view, starts Media Foundation, supplies its allocator and observes the
parent's exact NV12 luma byte in an `IMFSample` with the canonical duration. An
empty follow-up request remains non-blocking. This does not justify placing
Win32 code in `capyio-runtime`, broadening the audio-only `CapyIOBroker`, or
switching registered activation. Production ownership should use a dedicated
Windows camera-share boundary and a separate background camera host.

CAPY-CAMERA-001B1H implements that dependency direction. The COM crate imports
the consumer contract but no longer owns mapping creation/security calls. The
camera-share crate exposes fixed production names plus an optional test-only
API restricted to bounded local CapyIO names. The headless camera-host owns one
producer and deterministic release/retry state. A consumer may adopt the stream
ID and epoch only from the validated ACL-protected header. B1H still does not
provide a running service process, network/codec pipeline, Android Camera2
capture, registered shared activation or a LocalService host test.

CAPY-CAMERA-001C7 records the first Global/registered Android-frame roundtrip.
The initial run established that returning `MF_E_NOTACCEPTING` for a normal gap
between shared publications is pipeline-fatal under Frame Server. Registered
shared activation now reserves at most four FIFO requests on a Media Foundation
serial queue and uses a 5 ms scheduled callback until a newer validated frame
exists. It does not block the COM callback or duplicate a source sequence. The
repaired controlled run enumerated one Session/CurrentUser `CapyIO Camera` and
returned two advancing V2419A-backed NV12 samples. Exact rollback removed the
Session registration, hash-locked DLL, fixed CLSID and empty lab directories.

## References

- Microsoft `MFCreateVirtualCamera`:
  <https://learn.microsoft.com/en-us/windows/win32/api/mfvirtualcamera/nf-mfvirtualcamera-mfcreatevirtualcamera>
- Microsoft `IMFVirtualCamera::Start`:
  <https://learn.microsoft.com/en-us/windows/win32/api/mfvirtualcamera/nf-mfvirtualcamera-imfvirtualcamera-start>
- Microsoft `IMFVirtualCamera`:
  <https://learn.microsoft.com/en-us/windows/win32/api/mfvirtualcamera/nn-mfvirtualcamera-imfvirtualcamera>
- Microsoft `IMFVirtualCamera::Shutdown`:
  <https://learn.microsoft.com/en-us/windows/win32/api/mfvirtualcamera/nf-mfvirtualcamera-imfvirtualcamera-shutdown>
- Microsoft Frame Server custom media source guidance:
  <https://learn.microsoft.com/en-us/windows-hardware/drivers/stream/frame-server-custom-media-source>
- Microsoft Windows-Camera virtual-camera sample README:
  <https://github.com/microsoft/Windows-Camera/blob/master/Samples/VirtualCamera/README.md>
- Microsoft Windows-Camera `SimpleMediaSource` reference:
  <https://github.com/microsoft/Windows-Camera/blob/master/Samples/VirtualCamera/VirtualCameraMediaSource/SimpleMediaSource.cpp>
- Microsoft `IMFMediaStream::RequestSample`:
  <https://learn.microsoft.com/en-us/windows/win32/api/mfidl/nf-mfidl-imfmediastream-requestsample>
- Microsoft `MFScheduleWorkItem`:
  <https://learn.microsoft.com/en-us/windows/win32/api/mfapi/nf-mfapi-mfscheduleworkitem>
- Microsoft `IMFSampleAllocatorControl`:
  <https://learn.microsoft.com/en-us/windows/win32/api/mfidl/nn-mfidl-imfsampleallocatorcontrol>
- Microsoft `IMFMediaEventQueue` thread-safety contract:
  <https://learn.microsoft.com/en-us/windows/win32/api/mfobjects/nn-mfobjects-imfmediaeventqueue>
- Microsoft windows-rs:
  <https://github.com/microsoft/windows-rs>
