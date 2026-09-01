# CAPY-CAMERA-001 / CAPY-IO-CONTRACTS-001 — Parallel I/O foundation

Status: active

Owner: Codex

Created: 2026-08-29

Requirements: `FR-SCEN-003`, `FR-SCEN-004`, `FR-SCEN-006`, `FR-PORT-002..005`,
`NFR-STAB-002..004`, `NFR-MAINT-001..004`

## Objective

Establish one stable Video/Input semantic baseline from `main`, then begin a
hardware-free deterministic camera slice while the existing microphone work
continues independently.

## Slices

1. `CAPY-IO-CONTRACTS-001`: portable Video/Input crates, canonical Profile
   helpers, validation/tests and reserved non-functional workspace boundaries.
2. `CAPY-CAMERA-001A`: deterministic 720p30 NV12 color-bars/moving-clock
   producer and bounded frame queue; no phone or system registration.
3. `CAPY-CAMERA-001B0`: Windows 11 Media Foundation projection seam, dedicated
   ADR, SDK/tool inventory and read-only SoftwareCameraSource support probe. It
   cannot create or register a camera.
4. `CAPY-CAMERA-001B1A`: allocation-free one-stream Frame Server event/request
   core plus backend-neutral registrar orchestration and rollback tests. No
   Windows COM or registration backend exists in this slice.
5. `CAPY-CAMERA-001B1B`: windows-rs COM media source/stream, event queues,
   exact NV12 media type and local non-registered activation harness. Complete.
6. `CAPY-CAMERA-001B1C`: session/current-user `IMFVirtualCamera` backend and
   controlled application enumeration. Invoking registration, `Start`, stop or
   removal requires a separately approved command and recorded rollback. The
   COM class factory, required Frame Server interfaces, real backend and closed
   lab tool are implemented; the approved Frame Server two-frame roundtrip and
   exact rollback passed on the recorded host.
7. `CAPY-CAMERA-001B1D`: bounded cross-process sharing validation. One closed
   parent command owns the session/current-user registration while exactly two
   independently spawned consumer processes run sequentially; each enumerates,
   activates and pulls two frames through the public Media Foundation device
   surface. This is a validation harness, not the production
   Runtime/frame-ingress boundary, and it does not establish simultaneous
   multi-consumer fan-out.
8. `CAPY-CAMERA-001B1E`: bounded decoded-frame ingress seam. It validates and
   owns canonical NV12 frames, provides fixed drop-oldest buffering and proves
   non-registered Media Foundation projection. It adds no shared memory,
   network, codec or Android capture and does not change registered activation
   away from the fixture.
9. `CAPY-CAMERA-001B1F`: versioned Windows decoded-frame shared memory. One
   producer owns a fixed global mapping; multiple independent consumers open
   read-only views and take the newest committed frame. The gate includes a
   non-registered MF provider and cross-process tests, but does not yet connect
   Runtime/network/Android input or change registered activation.
10. `CAPY-CAMERA-001B1G`: non-registering cross-process MF composition. A
    producer process publishes one shared frame and an independent process
    opens the read view, constructs the MF source and verifies the platform
    sample bytes/timing plus non-blocking empty behavior.
11. `CAPY-CAMERA-001B1H`: extract the mapping into a dedicated Windows
    camera-share crate and add a headless producer lifecycle owner. MF consumes
    only the read side; the host owns start/publish/stop without adding a
    transport, codec, Android API or registered-source switch.
12. `CAPY-CAMERA-001C0`: build-only Android Camera2 lab after explicit
    permission authorization. One visible Activity requests CAMERA on a user
    action, previews and observes bounded YUV frames, then closes on pause. No
    APK install, device capture, service, storage or transport occurs.
13. `CAPY-CAMERA-001C1`: reviewed MediaCodec input/access-unit boundary and
    exact encoded-stream contract; still no network listener or background
    service.
14. `CAPY-CAMERA-001C2`: Camera2/MediaCodec composition and one explicitly
    authorized foreground device run. Select a common preview/encoder size,
    target both Surfaces, observe capture
    timestamps and bounded encoded/drop metrics; no network or image retention.
15. `CAPY-CAMERA-001C3`: versioned private AVC config/access-unit record with
    identical Android/Rust goldens, bounded Rust decoding and transactional
    stream/epoch/replay/discontinuity guard; no network or permission change.
16. `CAPY-CAMERA-001C4`: foreground-only Android CAVC exporter and loopback-only
    Windows validation receiver over an exact-device ADB reverse lab tunnel.
    Add the explicitly authorized INTERNET permission but no service, discovery,
    LAN listener, decoder or production transport claim.
17. `CAPY-CAMERA-001C5`: Windows user-mode Media Foundation H.264 decoder for
    guard-accepted Annex-B records, producing bounded packed NV12 only inside
    the loopback lab. No shared-memory publication, registration or driver.
18. `CAPY-CAMERA-001C6`: decoded-frame to camera-host publication, fixed Local
    cross-process lab probe and Global-only registered activation selection.
    No registration is invoked; privileged Global ownership and the system
    roundtrip remain separately approved gates.
19. `CAPY-CAMERA-001C7`: bounded asynchronous registered shared-source pump and
    controlled V2419A Global/Frame Server roundtrip with exact system rollback.
20. `CAPY-CAMERA-001C8`: fixed 60-second GUI hold and controlled Windows inbox
    Camera live-preview verification with no retained camera pixels.
21. `CAPY-CAMERA-001C9`: latency hardening through newest-frame bounded Android
    queues, explicit Android encoder hints, verified Windows decoder low-latency
    mode and backlog evidence; exact glass-to-glass measurement remains open.
22. `CAPY-CAMERA-001C10`: bounded same-clock Windows decoder and shared-publish
    stage timing with no CAVC change or cross-device clock subtraction.
23. `CAPY-CAMERA-001C11`: bounded Android front/back selection and lifecycle-safe
    stream restart with no permission or wire-contract change.
24. `CAPY-CAMERA-001C12`: bounded foreground connection retry when the fixed
    ADB-reverse receiver starts shortly after Android capture.
25. `CAPY-CAMERA-001C13`: bounded foreground AVC quality choices with
    lifecycle-safe stream restart and no wire or permission change.
26. `CAPY-CAMERA-001C14`: closed Windows live-hold orchestration with fixed
    receiver arguments, readiness/liveness gates and bounded cleanup.
27. `CAPY-CAMERA-001C15`: parameter-free read-only package/host cleanliness
    preflight for the exact C14 release artifacts.
28. `CAPY-CAMERA-001C16`: retain one Windows virtual-camera mapping across a
    bounded Android front/back stream restart and rebase resumed publications.
29. `CAPY-CAMERA-001C17`: bounded read-only Camera2 inventory for logical and
    physical lenses, focal/sensor metadata and concurrent groups.
30. `CAPY-CAMERA-001C18`: bounded foreground selection of one directly
    openable Camera2 device or one physical lens owned by a logical camera.
31. `CAPY-CAMERA-001C19`: V2419A-compatible logical-camera Zoom targets derived
    from bounded physical-lens focal metadata and advertised Zoom range.
32. `CAPY-CAMERA-001C20`: vendor-neutral directly openable Camera ID choices
    with automatic/minimum/1x/2x presets derived only from advertised Zoom.
33. `CAPY-CAMERA-001C21`: bounded monotonic encoded-progress watchdog and
    explicit camera local-lab MVP closure boundary.
34. `CAPY-CAMERA-001C22`: restore blocking mode on Windows reconnect sockets
    before bounded CAVC reads, closing front/back continuity on Windows.
35. `CAPY-CAMERA-001C23`: explicit ADB-free trusted-LAN lab endpoint. Preserve
    loopback as default; require one canonical private/CGNAT/link-local Windows
    IPv4 on Android and exact Windows bind/phone allowlist addresses on fixed
    port 38173. No DNS, wildcard, discovery, persistence or production-security
    claim.
36. `CAPY-CAMERA-001C24`: harden the visible foreground lab with a longer fixed
    startup/reconnect budget, authoritative cleanup errors and truthful
    Camera2-ID/vendor-Zoom labels without claiming physical-lens selection.
37. `CAPY-CAMERA-001C25`: retain one visible Camera2/encoder/export session
    across orientation and bounded display-size changes without locking
    orientation or weakening true foreground pause cleanup.
38. `CAPY-CAMERA-001C26`: allow a registered camera activated before its fixed
    producer mapping to emit a placeholder and attach later without reopening.
39. `CAPY-CAMERA-001C27`: bound registered live-producer stalls, resume the
    placeholder without timeline rewind and reattach a fresh or replacement
    producer while rejecting stale replay.
40. `CAPY-CAMERA-002`: reviewed VCamdroid-compatible external Adapter. Its
    H.264/RTSP data plane remains private `AdapterManaged` traffic.

Parallel worktrees after the contracts baseline has an approved commit:

- `codex/capyio-camera` — Camera fixture/projection;
- `codex/capyio-gamepad` — recorded IMU to deterministic DSU mapping;
- `codex/capyio-android-node` — platform-neutral module registry first;
- `codex/capyio-touchpad` — specification/fixtures until a slot opens.

## Current safety boundary

- no driver install/removal or APK removal; the camera lab APK was installed on
  one explicitly selected device under a separate authorization;
- only the explicitly authorized Android CAMERA and INTERNET declarations; no
  storage, microphone, location or foreground-service permission/service;
- C4 loopback/ADB reverse remains the default. C23 may open one explicitly
  selected trusted-LAN IPv4 listener only with an exact phone IPv4 allowlist;
  there is no wildcard, DNS, discovery or production transport claim;
- no persistent virtual-camera registration outside an explicitly approved,
  exact-target lab run; no driver deployment or system security change;
- no retained phone-camera pixels or personal video fixture;
- no upstream source import, FFmpeg/external codec or new third-party package;
  C5 uses the existing windows-rs binding and inbox Windows H.264 MFT;
- no commit, push or pull request without explicit human approval.

## Acceptance for the common baseline

- canonical Profile helpers match `PORT_PROFILES.md`;
- Video/Input validation covers bounds, exact negotiation, epoch/sequence gaps,
  stale/future data, fail-safe resets and neutral state;
- workspace marker crates compile while clearly reporting no implementation;
- full available Rust/document/manifest/Adapter checks pass;
- the microphone working tree remains unchanged.

## Progress

The common baseline passed the available validation and was committed locally
as `fc3da36` after explicit human approval. Camera, Gamepad, Android Node and
Touchpad worktrees now start from that exact commit. Camera 001A is complete.
Camera 001B0 adds the pure Media Foundation projection seam and a read-only
support probe; the local host reported SoftwareCameraSource support. Camera
001B1A now fixes source/stream event ordering, a four-request fixed FIFO and
registrar rollback semantics behind an in-memory backend. Camera 001B1B now
implements and exercises an in-process `IMFMediaSourceEx`/`IMFMediaStream2`
pair, separate thread-safe event queues and deterministic 2D NV12 samples. It
has no class factory, DLL export or registration backend. No virtual camera was
created or registered. Camera changes remain uncommitted and no branch has been
pushed.

Camera 001B1C now adds `DllGetClassObject`/`DllCanUnloadNow`, an `IMFActivate`
factory, required `IMFGetService`/`IKsControl`/allocator behavior, a Legacy
sensor profile, bounded COM lifetime tracking, the exact session/current-user
`IMFVirtualCamera` backend and a closed preflight/roundtrip/cleanup lab tool.
The approved elevated host run passed exact enumeration and two-frame Source
Reader validation. Cleanup then removed the session object, fixed CLSID,
hash-recorded DLL and empty ProgramData directories; final preflight found no
registration. The host required an administrator token despite camera privacy
access already being enabled.

Camera 001B1D adds a closed `shared-roundtrip` lab command. After exact
symbolic-link enumeration, it sequentially spawns two copies of the
hash-recorded lab executable with a fixed `consumer-probe` argument. The parent
passes its exact camera link through a bounded internal environment value. Each
consumer performs its own Media Foundation device enumeration and activation
and validates two frames. The parent imposes a 20-second deadline per child,
kills/reaps an unfinished child and always runs registrar stop/shutdown. No
executable path, camera name, CLSID, count or timeout is accepted on the command
line. Targeted tests and the sequential two-consumer host roundtrip pass. A
simultaneous dual-consumer probe still returned
`MF_E_HW_MFT_FAILED_START_STREAMING`; concurrent fan-out remains unresolved.

Camera 001B1E now adds `ExternalNv12FrameIngress`, binding one typed stream ID
and epoch to a fixed-capacity, drop-oldest queue. It rejects malformed payloads,
identity/epoch mismatches, non-advancing sequence/timestamps and end-of-stream.
The non-registered MF constructor can consume this queue without waiting on a
contended lock; queue gaps become `MFSampleExtension_Discontinuity` and an empty
queue returns `MF_E_NOTACCEPTING`. The in-process test proves that caller-owned
payload bytes, rather than regenerated fixture bytes, reach MF samples. The
registered class factory intentionally remains fixture-backed: a later slice
must add the explicit Frame Server process boundary before phone or network
frames can enter the system camera.

Camera 001B1F now supplies that first explicit process boundary. The fixed
`Global\\CapyIO.CameraIngress.v1` object has a versioned 4,147,648-byte
triple-slot layout, one creator/producer, read-only consumers, a protected DACL,
bound stream/epoch/generation metadata and stable commit checks around every
copy. Targeted tests cover duplicate ownership, independent readers, skipped-
frame discontinuity, non-blocking empty reads and a true child-process read.
The shared consumer is available to a non-registered MF constructor only. The
Runtime is not yet the producer and the registered class factory remains on the
deterministic fixture, so this gate does not yet make remote frames visible as a
system camera.

Camera 001B1G now proves the B1F mapping and MF source compose across a real
process boundary. The parent publishes a caller-owned frame, then directly
spawns the current test executable. The child opens the mapping read-only,
starts MF, supplies the video allocator and observes the exact shared luma byte
and canonical sample duration; a second request fails fast after the sole
publication is consumed. No registered activation or global production mapping
is used. Dependency review also confirms the platform-neutral Runtime must not
own Win32 mapping code and the existing `CapyIOBroker` remains audio-specific;
the production producer boundary should therefore be extracted into a
dedicated Windows camera-share crate and hosted by a separate background camera
component.

Camera 001B1H now extracts the B1F mapping implementation from the COM crate
into `capyio-windows-camera-share`. Production APIs remain fixed to the global
name; a feature-gated test API accepts only bounded local CapyIO test names.
`capyio-windows-camera-host` owns deterministic producer lifecycle and tests
inactive publish rejection, duplicate ownership, frame accounting, explicit
release and retry. The MF consumer can adopt the validated producer header's
stream ID/epoch without an external identity argument. Repository inspection
also confirms `codex/capyio-android-node` is still at the common contract
baseline and the repository has no Gradle project, Android manifest, Camera2,
MediaCodec or encoded-camera transport. Therefore Android-to-Windows live video
is not yet implemented.

Camera 001C0 now replaces the earlier no-Android-project state with a standalone
build-only Camera2 lab. Its manifest declares exactly CAMERA, the user must
press Start before the platform permission request, and Activity pause or
preview destruction closes capture. A two-image `YUV_420_888` reader observes
latest-frame metadata and closes every image immediately; it stores and sends
no pixels. The no-dependency lifecycle contract test, debug APK build and
strict Android lint pass. `aapt2` confirms the built APK has only CAMERA
permission. The APK has not been installed, ADB has not run and no phone image
has been captured, so device behavior and Android-to-Windows video remain
unverified.

Camera 001C1 now adds an Android-free AVC configuration/access-unit contract
and a real API-36 `MediaCodec` surface encoder owner. Configuration, payload,
codec-specific data and queue capacity are bounded; output callbacks perform a
single owned copy into a non-waiting drop-oldest queue and expose loss counts.
The Android APK build, strict lint and expanded contract tests pass. The
encoder is deliberately not yet wired into the Camera2 session and has not run
on a device, so actual codec availability, stream combinations and negotiated
AVC output remain unverified.

Camera 001C2 now attaches the MediaCodec input Surface to the repeating Camera2
request alongside the visible preview. It selects a common advertised
`SurfaceTexture`/`MediaCodec` size, records Camera2 sensor timestamps, drains at
most eight encoded outputs per capture result and displays only capture/
encoded/drop counters. The APK build and strict lint pass and its permission
set remains exactly CAMERA. On an explicitly selected Android 16 / API 36 vivo
device, Camera Service recorded clean connect/disconnect pairs and MediaMetrics
recorded a vendor AVC encoder producing 779 frames / 12,974,312 bytes for the
longer 1280x720, 4 Mbit/s session. Switching away left no active camera client.
This is phone-local capture/encode evidence, not Android-to-Windows sharing.

Camera 001C3 now fixes the first byte-level Android/Windows Adapter boundary.
Android Java and Rust produce the same version-1 config and key-frame records;
Rust rejects malformed lengths/fields/layouts and transactionally guards one
exact stream/epoch against data-before-config, replay, time regression, unmarked
gaps and invalid EOS. The record is private `AdapterManaged` data and no socket,
permission, service, decoder or APK update was added. Actual vendor layout
detection and authenticated delivery remain the next gates.

Camera 001C4 now connects real MediaCodec outputs to the CAVC record without
putting network work on Camera2 or codec callbacks. A pure-Java session encoder
detects vendor byte layout, suppresses codec-config buffers, waits for the first
key frame and marks loss recovery as a discontinuity. An eight-entry non-waiting
queue feeds a worker whose only target is Android loopback port 38173. The Rust
lab receiver binds Windows loopback, bounds each stream allocation and applies
the C3 guard. Contract, Rust, APK and lint builds pass locally and the manifest
contains exactly CAMERA plus INTERNET. After exact authorization, the
hash-matched APK was installed on V2419A and an ADB reverse mapping carried
Annex-B 1280x720 AVC into the Windows receiver. The guard accepted 90 access
units, including four key frames and 601,130 payload bytes, through last source
sequence 104. Cleanup force-stopped the app, Camera Service showed no active
client and the mapping was removed. This proves live vendor encoded bytes reach
Windows; Windows decode, packed NV12 publication and virtual-camera visibility
remain unverified.

Camera 001C5 now implements the Windows decoder half of that live path. The
Adapter accepts only the C3-guarded Annex-B layout, feeds bounded SPS/PPS plus
access units to the inbox Media Foundation H.264 transform, handles bounded
backpressure/type-change/flush/drain states and normalizes output to packed
NV12. A release-mode exact-device run accepted and decoded all 90 access units
into 90 changing 1280x720 frames. The app was force-stopped and the ADB reverse
mapping removed afterward. The decoded frames are still discarded by the lab;
camera-host shared-memory publication and registered virtual-camera consumption
are not part of C5.

Camera 001C6 now maps decoded source identity/timing/discontinuity into the
existing camera-host frame contract and provides a fixed-name independent
consumer probe. A config-only Local preflight opened the mapping in a separate
process; its expected zero-frame timeout and sender no-key-frame failure left
no persistent object. A matching production Global preflight established the
host boundary: the normal token received Win32 access denied, so production
ownership must move to a separately authorized privileged camera host/service.
Registered activation is ready to use only a validated Global mapping, falls
back to the fixture only when absent and fails other errors. A V2419A live Local
run then published 517 decoded 1280x720 NV12 frames; an independent process
opened the fixed mapping and observed 30 advancing frames from sequence 28
through 112 with distinct first/last checksums. Cleanup removed the reverse
mapping, camera client and both lab processes. The privileged Global/registered
roundtrip was retained as the next separately authorized gate.

Camera 001C7 completed that controlled gate. The first run exposed
`MF_E_NOTACCEPTING` as a fatal Frame Server error when a request arrived between
30 fps publications. A fixed four-request Media Foundation serial pump now
retains FIFO tokens and retries after 5 ms without blocking or duplicating
frames. The repaired V2419A run published 1,226 decoded NV12 frames through the
Global mapping; the system enumerated exactly one `CapyIO Camera` and Source
Reader returned two advancing samples. Final rollback removed the Session
camera, fixed CLSID, hash-locked DLL, reverse mapping, Android camera client and
all lab processes. Named ordinary-application compatibility and simultaneous
multi-application fan-out remain unproven.

Camera 001C8 closes the named ordinary-application gap. A parameter-free
60-second hold exposes only the same Session/CurrentUser `CapyIO Camera`; a
fresh Windows inbox Camera launch displayed the live V2419A workspace scene.
The receiver decoded and published 2,718 changing 1280x720 NV12 frames through
source sequence 3,040. The visible screenshot stayed conversation-only. Exact
rollback left no registration, fixed CLSID, deployed DLL, reverse mapping,
Android camera client or lab process. Simultaneous multi-application fan-out
and long-running lifecycle/reconnect behavior remain unproven.

Camera 001C9 responds to the first subjective latency observation. Android
encoded/export capacity falls from four/eight access units to two/two, both
overload paths retain newest work, and the encoder requests one-frame latency,
realtime priority, 30 fps input and zero B-frames. Windows enables and reads
back the inbox decoder's low-latency mode. Two V2419A runs published 3,600 and
3,562 frames with no observed decoder pending-sample backlog, and Windows inbox
Camera again showed continuous live pixels. This does not establish a numeric
end-to-end latency: the phone was not pointed at the prepared clock harness and
the Android vendor output-format latency value was not recovered. Exact
rollback again left no registration, fixed CLSID, deployed DLL, reverse mapping,
camera client or lab process.

Camera 001C10 localizes part of the remaining delay without changing the wire
contract. Two 300-frame V2419A runs measured the inbox H.264 decoder at roughly
2.4 ms average and below 7.7 ms maximum; the Local NV12 conversion/shared-memory
publication stage averaged 0.137 ms and remained below 0.521 ms. Both runs had
zero pending decoder samples. These same-clock measurements exclude Android
capture/encode, transport, Frame Server and display and therefore do not replace
the open clock-in-camera end-to-end test. No system camera was registered, and
cleanup left no reverse mapping, Android camera client or receiver process.

Camera 001C11 adds the first user-facing camera-source control. A pure bounded
policy selects the requested front/back facing with explicit fallback. Switching
while streaming fully closes the prior Camera2 device, encoder and exporter,
then starts a new session with a new CAVC stream identity/epoch; Activity pause
cancels any pending restart. Contract, offline APK assembly and API 29 lint pass,
and permissions remain exactly CAMERA and INTERNET. Physical switch verification
on V2419A observed Camera Service IDs `0 → 1 → 0`. Each source produced its own
90-frame 1280x720 decoded stream with a distinct stream ID and epoch. Cleanup
removed the reverse mapping and temporary UI inventory and left no camera client
or receiver process.

Camera 001C12 removes the single-connect startup race. Android now makes at most
20 foreground worker attempts with fixed timeout/delay while its two-entry
latest-frame queue remains non-waiting. Connect or bridge-write failure closes
the session; a fresh CAVC session encoder recovers at a later key frame.
Exhaustion is explicit, Activity close
interrupts retry, and the destination remains device loopback port 38173. Pure
contract, offline assemble/lint and repository validation pass. The newly
provided endpoint `100.66.157.119:40263` is verified as the same V2419A and is
clean. After exact authorization, the accepted whole-session retry kept Android
capture active for three seconds with no Windows listener, then recovered a
120-frame decoded stream at a later key frame with explicit discontinuity.
Cleanup removed the reverse mapping and left no camera client or receiver.

Camera 001C13 adds a minimal user-facing quality control without expanding the
transport or projection boundary. Economy, Balanced and Clear request 2, 4 and
6 Mbit/s at 1280x720 and scale by negotiated pixel count inside the existing
AVC bounds. A live change uses the proven full-close restart path, giving the
replacement encoder a new CAVC stream identity and epoch. Pure contract tests,
offline APK assembly, strict lint and repository validation pass; permissions
remain exactly CAMERA and INTERNET. After exact hash/target authorization,
V2419A produced separate 90/90-frame decoded Balanced, Clear and Economy
sessions at 4/6/2 Mbit/s. Each used a distinct stream ID and epoch; cleanup
removed the reverse mapping and left no camera client or receiver process.

Camera 001C14 adds a parameter-free Windows `live-hold` command around the
already verified receiver, Global mapping and Session/CurrentUser registrar.
It rejects ambient production mapping/registration state, spawns only the
fixed sibling receiver with fixed publication arguments, gates registration on
a validated mapping, checks liveness for the fixed 60-second hold and bounds
child/mapping cleanup. Hardware-free tests and static validation pass. The
command was not executed because it is a real system virtual-camera operation
requiring separate exact approval and a deployed hash-locked COM DLL.

Camera 001C15 adds a parameter-free, non-elevating PowerShell preflight for the
exact C14 package. It hashes all three release artifacts, cross-checks the DLL
deployment pin and rejects residual ProgramData, CLSID, TCP listener or lab
process state. The current host passed this read-only check with no system or
ADB mutation. Repository validation pins the script surface and artifact
hashes.

Camera 001C16 retains the Global publication mapping during a fixed five-second
Android stream-restart grace. The replacement CAVC identity receives a fresh
guard and decoder while the Adapter-owned virtual-camera publication timeline
continues monotonically and marks its first resumed frame discontinuous.
Focused and full repository validation pass; physical front/back regression is
pending new-hash approval.

Camera 001C17 adds a user-triggered read-only Camera2 inventory. Its bounded
Android-free model emits version-1 JSON for directly openable devices, logical
physical IDs, per-physical-lens focal/sensor data, Zoom range, common
preview/MediaCodec sizes and API-30 concurrent groups. Collection never calls
`openCamera`, creates no Surface and writes no file/log. Contract tests,
offline APK assembly and strict lint pass with unchanged CAMERA/INTERNET
permissions. Physical inventory evidence is pending a new ADB endpoint and
exact APK install authorization.

Camera 001C18 turns that bounded inventory into a foreground source selector.
One source is either a directly openable Camera ID or a physical lens addressed
through its owning logical camera. Physical selection applies the same physical
ID to the preview and encoder OutputConfigurations and otherwise preserves the
single-stream CAVC/restart contract. Contract tests, offline APK assembly and
strict lint pass with unchanged CAMERA/INTERNET permissions. The supplied
`100.66.157.119:46143` endpoint timed out and failed a TCP probe, so no APK was
installed and physical source evidence remains open.

The authorized C18 V2419A run later recovered the inventory and isolated the
physical-output behavior. Logical source `0` decoded 90 changing frames, while
`0/2`, `0/3` and `0/4` each stopped at one capture callback and zero encoded
units. Camera 001C19 therefore retains the logical camera outputs and converts
each physical focal length into an explicit, advertised-range-clamped
`CONTROL_ZOOM_RATIO` target. This should provide usable main/tele/ultra-wide
choices without claiming that the vendor selected a particular sensor.
Hardware-free contract, assembly and lint pass. The authorized exact-hash run
then completed separate 30-frame decoded streams for the 1.000x, 2.034x and
0.670x targets. Each used a fresh stream/epoch, produced changing checksums,
enabled decoder low-latency mode and had zero pending-sample backlog. These
results validate usable lens targets, not exact vendor physical-sensor
selection.

Camera 001C20 removes physical topology from the ordinary selection contract.
Every directly openable Camera ID retains an automatic choice and receives only
the supported subset of minimum-below-1x, 1x and 2x targets from its advertised
Zoom range. Physical IDs/focal metadata remain diagnostic inventory only. This
keeps the single-route baseline usable on vendors that hide physical IDs or
reject physical outputs. Contract tests, offline APK assembly and strict lint
pass. The authorized exact-hash V2419A run enumerated the expected automatic,
back 0.670x/1x/2x and front 1x/2x choices. Separate 0.670x and 2x runs each
decoded 30 changing 1280x720 frames with low-latency mode and no pending-sample
backlog. Cleanup stopped capture and removed the reverse mapping.

Camera 001C21 closes the last observed lifecycle ambiguity before feature
freeze. A one-second Handler check requires encoded progress inside a fixed
five-second window after the repeating request starts and after every encoded
unit. Expiry enters the existing failure/close path exactly once; close removes
the callback. Pure contract, offline APK assembly, strict lint and structural
validation pass. Product transport, pairing, installer and Runtime integration
remain outside the local-lab MVP.

Camera 001C22 closes the Windows reconnect defect exposed by the final C21
front/back regression. The listener is temporarily nonblocking only while it
polls inside the fixed five-second grace. Every accepted replacement stream is
restored to blocking mode before the existing bounded record reads; otherwise
Windows can inherit nonblocking mode and surface `WSAEWOULDBLOCK` (`10035`) as
a fatal CAVC error. A delayed-first-byte regression test pins this boundary.
The rebuilt receiver then completed one 60-second registered-camera lifetime
across Android Camera Service IDs `0 → 1 → 0`, accepting three distinct CAVC
streams and finishing all receiver, registration and mapping cleanup stages.

Camera 001C23 implements the first ADB-free development transport without
changing permissions, CAVC or background lifecycle. Blank Android input keeps
ADB reverse; a non-empty canonical private/link-local/100.64.0.0/10 Windows
IPv4 selects trusted-LAN mode. The receiver requires exact bind and phone peer
IPv4 literals together, fixed port 38173 and exact peer admission. The closed
Windows orchestrator exposes only those two address parameters and retains the
existing 60-second liveness and cleanup gates. Its authorized no-reverse run
accepted, decoded and Global-published 358 changing frames and enumerated the
temporary camera. Ordinary Windows Camera pixels and complete same-run cleanup
were not captured.

Camera 001C24 addresses that run's lifecycle defects without widening the
transport or permission model. Android now has 120 fixed connection attempts
and keeps the display awake only while visible capture is starting or active.
Windows allows 120 seconds for first mapping readiness and 60 seconds for a
replacement stream. Receiver and mapping cleanup failures are authoritative
instead of being hidden by an earlier validation failure. User-facing choices
now state that they are directly openable Camera2 IDs plus vendor Zoom targets;
a Zoom target can influence vendor lens selection but is not a physical-sensor
identity. Automated builds and focused tests pass; new exact artifacts require
separate physical authorization.

Camera 001C25 follows the authorized C24 run. C24 reached its trusted-LAN
config, validated Global mapping, one temporary-camera enumeration and complete
automatic Windows cleanup, but rotation recreated the Android Activity and
invoked the intentional foreground pause stop. C25 delegates orientation,
screen-size and smallest-screen-size changes to the existing Activity and
refreshes only UI state, preserving the in-memory endpoint and live session.
True pause and preview-surface destruction remain fail-closed. Offline build,
lint, permissions and structural validation pass. The authorized exact-hash
physical regression retained one Activity record and CAVC stream/epoch across
portrait/landscape/portrait, showed changing live pixels in ordinary Windows
Camera, released Camera2 on a true background transition, preserved the
unrelated reverse mapping and returned Windows deployment/process/port state to
a clean preflight. This closes the bounded local-lab rotation defect; production
pairing, service ownership and broad compatibility remain later work.

Camera 001C26 begins the next bounded Windows usability slice. Registered
activation no longer commits permanently to the deterministic fixture when the
fixed Global mapping is absent. A late-bound provider emits that fixture as an
offline placeholder, probes only the fixed mapping on the existing MF serial
queue after a fixed 15-placeholder-frame countdown, and rebases the first
validated live payload onto the active virtual-camera timeline with an explicit
discontinuity. Once live, it never interleaves placeholder frames. Focused tests
and structural validation cover late attachment and fail-closed invalid state;
system registration and physical late-phone evidence remain separately gated.

Camera 001C27 closes C26's one-way live handoff. Every registered activation
now enters the same asynchronous provider lifecycle. After 400 consecutive
empty 5 ms live polls, it releases its mapping handle, resumes the deterministic
fixture at the next virtual output sequence/timestamp and marks the fallback
discontinuous. The fixed 15-placeholder-frame probe then accepts a newer frame
from the paused producer or a new producer generation. Remembered pre-rebase
source identity/sequence/timestamp rejects replay of the old last publication.
Focused tests cover resumable fixture timing, producer exit, same-name mapping
replacement, stale replay and fresh reattachment. Exact release artifacts are
hash-pinned and the read-only preflight/full CI pass; ordinary Windows Camera
three-transition evidence remains separately authorized.

Camera 001C28 starts the remaining lifecycle work at the ownership boundary.
Android Camera2 capture can now be created without an Activity `TextureView`,
while a pure contract keeps service-owned capture alive across Activity
pause/resume and configuration changes. No service or new Android permission is
declared in this slice, so foreground behavior is unchanged and background
continuity is not yet claimed. The Windows lab hold is extended to a fixed 180
seconds, and locked COM-DLL cleanup has an explicit FrameServer stop/restart
action with `finally` recovery. Focused contract/Rust tests and structural
validation pass; APK assembly is currently blocked by an environmental
`AccessDeniedException` on generated `R.jar`. Foreground-service wiring plus
physical background/rotation recovery remain separately gated.

Camera 001C29 wires that ownership boundary into Android. A user-visible start
launches one unexported `camera` foreground service with an ongoing Stop
notification; that service owns the headless Camera2 encoder and exporter.
Activity pause/task removal/configuration changes are no longer stop events.
The service is non-sticky and stores no destination, so process/device restart
does not silently reactivate capture. The APK declares the separately approved
foreground-service permissions, builds offline and passes strict lint. Exact
C29 APK SHA-256 is
`8487317735E6EBDBB45B0AC4938826B6EEB529BA5F6FEBC3E31EDEA46E62E2B0`.
Physical installation and background/rotation/Windows-pixel evidence remain
pending a post-hash explicit approval.

Camera 001C30 installed the service-owned build and proved changing pixels in
ordinary Windows Camera plus same-PID foreground-service/Camera2 continuity
across Home and rotation. It exposed sensor-native pixels displayed 90 degrees
counter-clockwise and a 3,600-unit receiver ending before the advertised
180-second hold. Exact rollback, including FrameServer restart, passed.

Camera 001C31 carries a closed clockwise sensor-display rotation in compatible
AVC record v1.1 and rotates decoded NV12 before fixed-profile publication.
Portrait content is aspect-fitted with black pillarboxes. The live bound is now
7,200 units, giving 240 seconds at 30 fps around the 180-second GUI hold. Focused
Rust tests, Android contract/build/lint and clean preflight pass; the new exact
hashes require final physical authorization.
