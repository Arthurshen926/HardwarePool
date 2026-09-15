# CapyIO Testing Strategy

## Principles

- Tests prove explicit requirements and failure isolation.
- Core/Protocol/Adapter DTO tests run without hardware.
- Platform and driver claims require identified environments and retained
  evidence.
- Mock UI/Sidecar behavior is visibly simulated.
- Tests are not removed or weakened to hide migration defects.

## Unified commands

```text
cargo xtask doctor
cargo xtask fmt
cargo xtask check
cargo xtask test
cargo xtask validate-docs
cargo xtask validate-manifests
cargo xtask adapter-smoke
cargo xtask ci
cargo xtask demo
cargo xtask imu-demo
cargo xtask android-check
cargo xtask android-doctor --serial <explicit-serial>
cargo xtask android-baseline --serial <explicit-serial>
cargo xtask android-collect --serial <explicit-serial>
```

Frontend uses `corepack pnpm typecheck` and `corepack pnpm build`.

## Foundation unit tests

- Node has no global role and may own Source/Sink Ports;
- Capability/Adapter ownership and duplicate IDs;
- Source→Sink Profile compatibility;
- Source→Source, Sink→Sink and mismatched Profile rejection;
- valid/invalid Route transitions;
- opposite-direction Routes coexist;
- stopping one Route leaves another active;
- Adapter failure affects owned Routes only;
- catalog replacement after Adapter restart;
- Protocol catalog/Route/Problem round trips and enum/version failures;
- Adapter manifest validation;
- NDJSON framing, malformed/oversized messages and correlation;
- stdout/stderr limits are enforced while reading, including oversized input
  without a newline;
- timeout, late/unexpected response, malformed response and stdout closure
  poison the sequential Host, reap the child and reject later requests;
- generic Route prepare/start/stop/status contracts round trip without carrying
  continuous data payloads;
- child stdout machine-only behavior and bounded/truncated stderr;
- Requirement parser rejects duplicate, malformed and non-canonical IDs, and
  traceability covers every normative PRD ID;
- deterministic UI snapshot with four Routes.

## Fixture-first IMU tests

- committed JSONL envelopes validate Profile, timestamps, clock domain, epoch,
  sequence, SI units, coordinate frame, accuracy, calibration and sensor data;
- Panel and Recorder consume independent bounded queues from one fan-out;
- a full/stopped Recorder does not block Panel progress;
- gaps, duplicates, late samples, wrong streams, stale/future epochs, sequence
  exhaustion and recorder bounds have explicit regression tests;
- `cargo xtask imu-demo` replays the same compiled fixture through the headless
  node and emits numeric Panel plus JSONL Recorder evidence;
- the desktop Browser Mock and Tauri backend expose the same schema-v3 fixture
  summary and label it as simulated rather than live phone data.

These tests require no phone and make no SensorServer, APK, network or physical
timing claim.

## Video and input contract tests

- canonical video and input/haptics Profile helpers match the normative
  registry and replace stale mock-only names;
- packed NV12/BGRA candidates validate dimensions, reduced rational frame
  rate, closed colorimetry and derived payload bounds;
- negotiation requires exact complete candidates and performs no implicit
  resize, rotation, decode, color conversion or QoS rewrite;
- frame descriptors reject zero/stale epochs, wrong payload sizes and invalid
  end-of-stream payloads;
- input stream descriptors keep clock-domain allocation out of per-frame
  headers;
- the shared sequence guard rejects wrong stream, stale/future epoch,
  duplicate/late sequence, non-advancing epoch and sequence exhaustion, while
  reporting gaps;
- pointer/keyboard reset, empty touch snapshots, fixed gamepad neutral state and
  explicit haptics stop make fail-safe cleanup testable.
- the distinct touchpad Profile enforces positive himetric size, 3..=5 contacts,
  stable unique IDs per snapshot, declared geometry/pressure and closed button
  semantics;
- touchpad frame tracking suppresses initial, post-gap and post-epoch updates
  until explicit cancel-all, and timestamp validation is transactional;
- the committed synthetic fixture replays cancellation, one/two/four/five
  contacts, release and final cancellation with exact bounded metrics;
- the appended `Touchpad` Capability class round-trips as protocol enum value
  16 while touchpad frames stay outside the control envelope.

These tests prove only deterministic semantic contracts. They do not capture a
camera, inject OS input, register a virtual camera, start DSU/VIIPER, install a
driver/APK or prove ordinary-application compatibility.

## Windows camera fixture and projection contract tests

- the canonical fixture deterministically renders bounded 1280x720 30 fps NV12
  frames with rational timestamps and a pinned diagnostic checksum;
- the owned-frame queue enforces both frame and payload-byte limits, with
  explicit reject-newest or drop-oldest behavior and discontinuity evidence;
- the first Media Foundation plan can represent only session lifetime and
  current-user access with a bounded friendly name;
- sample timing maps an absolute source timestamp delta onto a QPC-correlated
  100 ns anchor and rejects wrong streams/epochs, duplicates and regressions;
- packed NV12 rows copy into a checked positive-stride destination and padding
  is cleared before the buffer can be published;
- first start/restart produce the required new/updated-stream, stream-started
  and source-started action order for the single canonical stream; a repeated
  start while already active preserves generation, pending requests and sample
  timeline state while producing updated/start events;
- sample requests use a four-entry fixed FIFO, reject overflow/out-of-order
  completion and remain pending after a transactional sequence failure;
- failure cancellation removes only the oldest accepted request and leaves the
  source started so the next pull can recover;
- stop/shutdown cancel outstanding requests, and registrar tests prove
  start/stop failures trigger terminal cleanup or an explicit cleanup-required
  state against an in-memory backend;
- the optional host probe calls only `MFIsVirtualCameraTypeSupported`;
- the Windows-only in-process COM test verifies source/stream attributes, the
  Legacy profile and provided-allocator contract, exact NV12 media type,
  first-start/restart event ordering, token identity, 2D buffer size/content,
  rational sample timing, pause/resume, stop and terminal shutdown behavior;
- class-factory tests verify the fixed CLSID, aggregation rejection, unsupported
  interface failure, server locking and unload only after source/stream release;
- backend contract tests prove the default object is inert, cleanup is
  idempotent and the plan remains session/current-user only;
- the opt-in `preflight` command calls `MFCreateVirtualCamera` and shutdown but
  never `Start`; the separately approved `roundtrip` command requires one
  enumeration match, two valid 30 fps NV12 samples through Source Reader and
  explicit cleanup. It bounds empty live-source reads, requires exact sample
  duration plus monotonic bounded downstream timestamps, and copies opaque
  downstream buffers into bounded CPU memory before checking NV12 luma.
- the separately approved `shared-roundtrip` keeps registration ownership in
  one parent and sequentially launches exactly two independent
  `consumer-probe` processes. Each child receives the parent's exact symbolic
  link through a bounded internal environment value, performs its own
  `MFEnumDeviceSources`/`IMFActivate` path and pulls two validated frames. The
  parent applies a separate 20-second deadline to each child, reports non-zero
  child exits and kills/reaps an unfinished child before registrar cleanup;
  command parsing rejects caller-supplied paths, identities, counts, timeouts
  and extra arguments. The host lab also records that simultaneous dual
  consumers still fail, so this gate does not claim concurrent fan-out.
- the 001B1E external ingress tests bind one stream/epoch, reject malformed,
  wrong-stream, wrong-epoch, non-advancing and end-of-stream frames, and prove
  the fixed drop-oldest bound plus discontinuity marking. The Windows
  in-process COM test feeds three caller-owned frames into a two-frame ingress,
  observes only the retained payload values in Media Foundation samples,
  verifies `MFSampleExtension_Discontinuity`, exact timing and a non-blocking
  `MF_E_NOTACCEPTING` result after the queue drains. This does not exercise a
  cross-process mapping or registered external-frame source.
- the 001B1F shared-ingress tests pin the 256-byte header, 64-byte slot header,
  three 1,382,464-byte slots, 4,147,648-byte total mapping and exact protected
  SDDL. They reject a second producer for the same name, prove two independent
  read-only consumers observe the same latest payload, mark a skipped
  publication discontinuous and spawn a separate test process that opens the
  mapping and verifies owned payload bytes. A provider-level MF test proves the
  shared consumer supplies the latest frame and returns
  `MF_E_NOTACCEPTING` without blocking once that publication is consumed. Test
  mappings use unique `Local\\...test...` names and vanish with their handles;
  they do not create the production `Global\\CapyIO.CameraIngress.v1` object or
  alter registered activation.
- the 001B1G composition test keeps the producer in its parent process and
  directly spawns the current test executable as an independent MF consumer.
  The child opens the unique mapping read-only, starts Media Foundation,
  supplies the platform video allocator, requests a sample, observes the
  parent's exact luma byte and canonical 333,333-tick duration, then verifies a
  second request fails fast with `MF_E_NOTACCEPTING`. This proves decoded bytes
  cross the process boundary and reach an MF sample; it does not use the
  registered class factory or Frame Server service token.
- the 001B1H extraction runs the fixed-layout, duplicate-owner, independent-
  reader and raw child-process tests in `capyio-windows-camera-share`, while the
  cross-process MF sample test remains in `capyio-windows-camera-mf`. Bounded
  test-only mapping-name APIs reject names outside the fixed local test
  namespace. Camera-host tests prove stopped publish rejection, start/publish/
  stop accounting, mapping disappearance after final release, idempotent stop,
  duplicate-owner failure without false activation and successful retry after
  the first owner releases. MF and host consumers also verify they can adopt
  the validated producer-bound stream ID and epoch.

The COM test creates only directly referenced objects inside its process after
`MFStartup`; it does not expose a class factory or call a virtual-camera API.
These normal tests and the probe do not register, enumerate, start, stop or
remove a system virtual camera. Approved host-specific lab evidence may
establish Frame Server delivery, but does not establish broad
ordinary-application compatibility.

Normal CI never deploys the COM DLL, writes HKLM, changes an ACL or invokes the
roundtrip/shared-roundtrip/cleanup system commands. Those results belong in the
host-specific 001B1C/001B1D/001B1E reports with the exact DLL hash and rollback
targets. The 001B1F mapping tests are ephemeral process tests and require no
registry or filesystem rollback.

The 001B1E release regression may repeat the approved fixture-backed system
`roundtrip` because the common `RequestSample` implementation changed. That
regression verifies no loss of the existing virtual-camera path; it does not
turn the process-local external ingress test into cross-process evidence.

## Android Camera2 build-only tests

CAPY-CAMERA-001C0 uses the standalone project below without installing an APK:

```text
cd platform/android/capyio-camera-app
gradle contractTest :app:assembleDebug :app:lintDebug
```

`contractTest` has no test-library dependency. It verifies explicit permission
request/grant/deny, visible start, pause-driven close, failure cleanup, retry
and bounded frame metadata. Android lint runs with warnings as errors; only the
repository-audited API 36 pin checks are disabled because changing the Android
toolchain pin is a separate task.

The APK manifest is then inspected with `aapt2 dump permissions`; the expected
and observed set is exactly `android.permission.CAMERA`. Normal repository
validation parses the source manifest offline, rejects any additional
permission or service declaration and checks the visible permission/pause/
secure-window controls. These checks do not prove a camera opens on a phone,
that device stream combinations work, or that an encoded frame reaches
Windows.

CAPY-CAMERA-001C1 extends the same no-dependency contract executable with AVC
configuration bounds, owned/read-only payload behavior, codec-parameter-set
bounds and deterministic drop-oldest queue behavior. `assembleDebug` and
strict lint compile the real Android `MediaCodec` owner against API 36. These
are compile/contract checks only: they do not instantiate a device codec,
connect its input Surface to Camera2, or assert a vendor encoder's negotiated
profile, level, color metadata or output framing.

CAPY-CAMERA-001C2 compiles and lints the Camera2/MediaCodec composition. Offline
validation requires common-size selection, the encoder Surface request target
and capture callback to remain present. One separately authorized Android 16 /
API 36 device run validated the advertised two-Surface combination, 1280x720
vendor AVC output and pause-driven camera release. OS MediaMetrics recorded 779
encoded frames for the longer session. Application-level codec-configuration,
key-frame and queue-drop evidence still needs a deterministic, privacy-safe
observation path before transport work can treat the stream contract as closed.

CAPY-CAMERA-001C3 adds no device or network test. Android Java and Rust assert
identical config/key-frame golden records. Rust round-trips those records and
rejects truncation, oversized input, wrong magic/version/kind/header/reserved
fields, mismatched lengths, invalid layouts, wrong stream/epoch, replay,
timestamp regression, unmarked gaps and invalid EOS. Guard rejection is
transactional. The Android executable test compiles main and contract-test
sources together because the installed JDK 25.0.2 has a Windows ZipFS close
defect when a freshly built classpath JAR is used; this changes no production
code or dependency.

CAPY-CAMERA-001C4 adds pure-Java layout/session tests covering Annex-B and
length-prefixed access units, AVC decoder-configuration records, config-first
output, codec-config-buffer suppression and loss recovery only at a key frame.
Rust adds a streaming reader test for concatenated records, clean record-boundary
EOF, truncated header/payload and oversized pre-allocation rejection. The lab
receiver accepts at most one loopback connection, applies the existing CAVC
guard and succeeds only after at least one key frame. The Android app now
compiles the contract source set directly rather than consuming its temporary
JAR; this avoids the installed JDK 25.0.2 Windows ZipFS close defect without
changing the APK contract. In the Codex Windows sandbox the same defect also
affects Android's generated `R.jar`, so the offline Gradle verification is run
outside the filesystem sandbox and remains network-disabled.

The C4 APK manifest is expected to contain exactly `android.permission.CAMERA`
and `android.permission.INTERNET`, with no service. Device transport evidence
requires all of the following after exact authorization:

```text
adb -s <exact-target> reverse tcp:38173 tcp:38173
cargo run -p capyio-vcamdroid-adapter --bin capyio-avc-lab-receiver -- --max-access-units 90
adb -s <exact-target> install -r <hash-recorded-debug-apk>
```

The Activity must remain visible and the user must press Start. The explicitly
authorized V2419A run installed the exact hash-recorded APK, established only
the 38173 mapping and delivered Annex-B 1280x720 AVC config plus 90 access units
through the receiver guard. Four were key frames; accepted payload totaled
601,130 bytes and ended at source sequence 104. The app was then force-stopped,
Camera Service reported `Active Camera Clients: []`, and the reverse list was
empty after removal. This proves real vendor AVC configuration/access units
crossed the ADB tunnel and passed framing/order checks; it does not prove
decode, NV12 delivery or Windows virtual-camera visibility.

CAPY-CAMERA-001C5 unit tests cover its Annex-B-only config, even bounded NV12
layout and CSD start-code rejection. The Windows lab adds an explicit decoder
mode:

```text
cargo run --release -p capyio-vcamdroid-adapter --bin capyio-avc-lab-receiver -- --decode-nv12 --max-access-units 90
```

The release build is required for the physical evidence so debug-mode hashing
does not create artificial socket backpressure. On the same authorized V2419A
and exact 38173 ADB reverse mapping, the receiver accepted 90 Annex-B 1280x720
access units, including three key frames and one marked discontinuity, and
decoded all 90 into 124,416,000 packed NV12 bytes. First/last FNV-1a checksums
were `93a1c21993ea4869` and `f8ed18c181c4113e`; the last decoded source sequence
was 91. Distinct checksums prove changing decoded buffers, not semantic camera
content. Cleanup again force-stopped the app, left no active camera client and
removed the reverse mapping. Shared-memory publication and public virtual-camera
consumption remain separate gates.

CAPY-CAMERA-001C6 adds an exact decoded-to-`VideoFrameDescriptor` test, retains
the camera-host duplicate-owner/lifecycle tests and adds a fixed independent
read-only probe. A config-only local-lab preflight proved a separate process can
open `Local\CapyIO.CameraIngress.v1.lab` and adopt stream
`00010203-0405-0607-0809-0a0b0c0d0e0f`, epoch 2. Since no key frame was sent,
the probe correctly timed out at zero frames and both processes failed closed;
the mapping disappeared on owner exit. A later V2419A live run published 517
decoded 1280x720 NV12 frames while an independent process observed 30 advancing
frames from sequence 28 through 112. Its first/last checksums were
`e6cbbef51d37c0a9` and `a36484d4d874d600`, and cleanup left no camera client,
reverse mapping or lab process. The matching Global preflight failed at
`CreateFileMappingW` with Win32 error 5 under the normal user token, and the
probe saw file-not-found. This is privilege-boundary evidence, not a failed
data contract. The controlled Global/registered roundtrip requires a separately
authorized privileged host and exact rollback.

CAPY-CAMERA-001C7 reproduces the registered-path `MF_E_NOTACCEPTING` failure
seen when Frame Server requested a second sample between live 30 fps
publications. Its regression test consumes one shared frame, accepts another
request while the mapping has no newer frame, publishes a distinct frame 20 ms
later and observes its exact luma through the retained FIFO request. Focused
tests and Clippy pass. On `DESKTOP-AT8EVE9`, the repaired hash-locked DLL then
used V2419A stream `ef2dc243a7c6d37fe8cb9a8cdd69521e`: the Global receiver
published 1,226 decoded NV12 frames, the Session/CurrentUser camera enumerated
exactly once as `CapyIO Camera`, and Source Reader returned two 1,382,400-byte
samples with 333,333-tick duration and a positive 666,610-tick delta. Final
checks found no camera registration, CLSID, deployed DLL, reverse mapping,
Android camera client or lab process.

CAPY-CAMERA-001C8 adds a parameter-free `gui-hold` command that retains the
same Session/CurrentUser camera for exactly 60 seconds after verifying one exact
enumeration match. On `DESKTOP-AT8EVE9`, a fresh Windows inbox Camera launch
displayed live V2419A workspace pixels through `CapyIO Camera`. The successful
stream `bd117f7226a47444b86eb1ce6c9d75a2` produced 2,718 decoded and published
1280x720 NV12 frames through source sequence 3,040 with distinct first/last
checksums. The screenshot was conversation-only and no camera pixels were
retained. Final preflight, filesystem, registry, process, ADB reverse and
Android Camera Service checks were clean.

CAPY-CAMERA-001C9 reduces the Android encoded and network-export queue bounds
from four/eight access units to two/two and makes both queues drop oldest under
overload. The Android encoder requests one-frame latency, realtime priority,
the selected maximum frame rate and zero B-frames; the foreground UI reports
the codec name and any latency value present in the output format. The Windows
H.264 decoder enables and reads back `CODECAPI_AVLowLatencyMode`, and its focused
test and warnings-denied Clippy pass. Two controlled V2419A runs decoded and
published 3,600 and 3,562 frames with `decoder_low_latency=true` and a maximum
pending decoder sample count of zero; the second run displayed continuous live
pixels in Windows inbox Camera. These are stage/backlog observations, not an
end-to-end latency measurement: the phone was not aimed at the prepared clock
harness, and the vendor encoder's output-format latency value was not recovered.
Final deployment, registration, process, reverse and Camera Service checks were
clean. See `docs/CAPY_CAMERA_001C9_REPORT.md`.

CAPY-CAMERA-001C10 adds bounded same-clock stage timing without changing CAVC.
The Windows decoder retains one monotonic submission instant per already-bounded
pending sample and aggregates count/average/maximum through saturating counters;
the shared publisher independently measures NV12 conversion plus mapping write.
A V2419A decode-only run processed 300/300 frames with 2,413 microseconds average
and 7,682 microseconds maximum decoder time. A second 300-frame Local mapping run
measured 2,399/7,528 microseconds average/maximum decoder time and 137/521
microseconds average/maximum shared-publication time. Both had zero maximum
pending decoder samples. These measurements exclude Android capture/encode,
transport residence, Frame Server and display, so they are not end-to-end
latency evidence. Cleanup left no reverse mapping, camera client or process. See
`docs/CAPY_CAMERA_001C10_REPORT.md`.

CAPY-CAMERA-001C11 adds a pure 32-candidate-bounded front/back facing policy and
a visible Activity switch. A running switch first follows the existing full
close lifecycle and then creates a new Camera2/MediaCodec/export session, so a
source-facing change cannot occur inside one CAVC stream epoch. Contract tests
cover toggle, exact selection, fallback and bounds. Offline Gradle contract,
assemble and warnings-as-errors lint pass at the unchanged API 29 minimum. The
rebuilt APK still declares exactly CAMERA and INTERNET. Physical front/back
switch evidence on V2419A then observed Camera Service IDs `0 → 1 → 0`; each
source produced a separate 90-frame 1280x720 stream with distinct stream ID and
epoch, key frames, changing decoded checksums and zero decoder backlog. Cleanup
left no reverse mapping, camera client, UI XML or receiver. See
`docs/CAPY_CAMERA_001C11_REPORT.md`.

CAPY-CAMERA-001C12 adds finite foreground retry when Android capture starts
before the fixed ADB-reverse receiver. A pure policy fixes 20 attempts, 500 ms
connect timeout and 500 ms retry delay. Socket work remains on the worker;
Camera2/MediaCodec callbacks retain only non-waiting two-entry drop-oldest
offers. Both connect and bridge-write failure consume a bounded attempt; a fresh
transport-free session encoder replays config only when a later key frame makes
recovery valid. Contract, offline assemble/lint and repository validation pass. The
rebuilt APK retains exactly CAMERA and INTERNET. Physical late-receiver evidence
on V2419A then started Android capture while Windows port 38173 had no listener,
kept it absent for three seconds, and subsequently decoded 120/120 frames after
the receiver appeared. Source sequence 211 plus one discontinuity evidenced
latest-key-frame recovery. Cleanup left no reverse mapping, camera client or
receiver process. See `docs/CAPY_CAMERA_001C12_REPORT.md`.

CAPY-CAMERA-001C13 adds a pure three-choice AVC quality policy and a visible
Activity control. At 1280x720 the Economy/Balanced/Clear choices request
2/4/6 Mbit/s respectively; smaller negotiated sizes scale by pixel count while
remaining inside the existing encoder bounds. A running change fully closes
and restarts Camera2, MediaCodec and export so the new encoder config receives
a new stream identity and epoch. Contract, offline assemble/lint and repository
validation pass. Permissions remain exactly CAMERA and INTERNET. The rebuilt
APK was installed after exact hash/target authorization. V2419A then produced
independent 90/90-frame decoded Balanced, Clear and Economy sessions at the
requested 4/6/2 Mbit/s, each with a distinct stream ID and epoch. Cleanup left
no reverse mapping, camera client or receiver. See
`docs/CAPY_CAMERA_001C13_REPORT.md`.

CAPY-CAMERA-001C14 adds the parameter-free Windows `live-hold` orchestration
command. Unit tests pin its closed command grammar, exact sibling receiver
arguments and retry-only-on-`ERROR_FILE_NOT_FOUND` mapping rule. The command
rejects pre-existing production mapping/registration state, waits at most 30
seconds for the receiver-owned validated Global mapping, then checks child and
mapping liveness throughout the existing fixed 60-second virtual-camera hold.
Success and failure paths explicitly stop/reap the child, run registrar
Stop/Shutdown and require mapping removal within five seconds. Rust tests,
Clippy and repository validation pass. No system registration, COM deployment,
ADB operation, Android capture or physical camera run occurred in this slice;
executing `live-hold` requires separate exact system-operation approval. See
`docs/CAPY_CAMERA_001C14_REPORT.md`.

CAPY-CAMERA-001C15 adds a parameter-free read-only package/host preflight. It
requires exact SHA-256 matches for the C14 receiver, orchestration executable
and COM DLL, verifies the deploy script pins the same DLL, and rejects an
existing CapyIO ProgramData root, fixed CLSID, TCP 38173 listener or named lab
process. The current host run passed all checks without elevation or mutation.
The script intentionally does not connect ADB or infer a device target. Static
repository validation pins its closed surface and all hashes. See
`docs/CAPY_CAMERA_001C15_REPORT.md`.

CAPY-CAMERA-001C16 closes the live front/back-switch transport gap found during
the first Windows inbox Camera run. That run visibly displayed changing V2419A
pixels through `CapyIO Camera`, but the Android switch correctly ended its CAVC
stream epoch and the single-connection receiver exited, which caused
`live-hold` to remove the temporary camera. The fixed `live-hold` still has no
caller-controlled parameters, but its exact sibling receiver arguments now add
a fixed five-second reconnect grace. During that grace the receiver retains the
same validated Global mapping and publisher, accepts a new loopback-only CAVC
session, creates a fresh decoder, rebases publication sequence/timestamps onto
the continuous Adapter-owned virtual-camera stream and marks the first resumed
frame discontinuous. Ordinary receiver runs remain single-connection by
default. Focused tests cover bounded reconnect acceptance, monotonic resumed
publication timing and the exact closed argument vector. A repeat physical
front/back switch remains gated on explicit approval of the new release hashes.
See `docs/CAPY_CAMERA_001C16_REPORT.md`.

CAPY-CAMERA-001C17 adds a bounded Android-free camera inventory model plus a
read-only Camera2 collector. Contract tests cover JSON escaping, logical/
physical lens consistency, focal/sensor/Zoom bounds, concurrent-group bounds
and malformed orientation. Offline API-29 APK assembly and warnings-as-errors
lint pass. `aapt2` confirms the debug APK still declares exactly CAMERA and
INTERNET. The collector never opens a CameraDevice or image Surface and does
not persist/log the inventory. Exact V2419A physical and concurrent-camera
evidence remains pending a fresh ADB endpoint and separately authorized APK
installation. See `docs/CAPY_CAMERA_001C17_REPORT.md`.

CAPY-CAMERA-001C18 adds a bounded Android-free source-selection contract and a
foreground Camera2 boundary that can address either a directly openable Camera
ID or one physical lens through its logical owner. Contract tests cover stable
source keys, deterministic cycling, focal metadata and malformed IDs. Offline
API-29 APK assembly and warnings-as-errors lint pass; `aapt2` confirms unchanged
CAMERA/INTERNET permissions. The supplied V2419A endpoint
`100.66.157.119:46143` was TCP unreachable, so no APK was installed and no
physical output configuration has yet been claimed as working. See
`docs/CAPY_CAMERA_001C18_REPORT.md`.

CAPY-CAMERA-001C19 records that the authorized V2419A C18 run decoded 90
changing frames from logical source `0`, while direct physical sources `0/2`,
`0/3` and `0/4` each stalled at one capture callback and zero encoded units.
The replacement contract derives bounded Zoom targets from focal metadata and
clamps them to the logical camera's advertised range. The Camera2 request uses
ordinary logical preview/encoder outputs and applies `CONTROL_ZOOM_RATIO` only
on API 30+. Contract tests, offline assembly and warnings-as-errors lint pass.
The exact-hash V2419A run then decoded 30 changing 1280x720 frames for each
1.000x, 2.034x and 0.670x target with low-latency mode enabled and no pending
sample backlog. This proves the general logical-camera Zoom path, not exact
physical-sensor attribution. See `docs/CAPY_CAMERA_001C19_REPORT.md`.

CAPY-CAMERA-001C20 removes physical IDs and focal ratios from the ordinary
selection path. Pure contract tests prove every directly openable Camera ID has
an automatic choice and only the supported subset of minimum-below-1x, 1x and
2x presets, with stable labels, no duplicates and a fixed maximum. Offline
API-29 assembly and warnings-as-errors lint pass; `aapt2` confirms unchanged
CAMERA/INTERNET permissions. The exact-hash V2419A UI enumerated automatic,
back 0.670x/1x/2x and front 1x/2x choices exactly once. Separate minimum and 2x
runs each decoded 30 changing 1280x720 frames with low-latency mode enabled and
no pending-sample backlog. See `docs/CAPY_CAMERA_001C20_REPORT.md`.

CAPY-CAMERA-001C21 adds a pure monotonic encoded-progress watchdog with a fixed
one-second check and five-second timeout. Tests pin the exact pre-timeout and
timeout boundary and reject invalid timestamps. Structural validation requires
the Android Handler schedule, encoded-unit progress update, one explicit stall
failure and close-time callback removal. Offline API-29 assembly and strict lint
pass with unchanged permissions. See `docs/CAPY_CAMERA_001C21_REPORT.md`.

CAPY-CAMERA-001C22 covers the Windows-specific accepted-socket mode inherited
after the reconnect listener becomes nonblocking. Its loopback test connects
inside the fixed grace but deliberately delays the first byte; the accepted
stream must block until that byte arrives rather than returning Windows socket
error 10035. Structural validation pins both the nonblocking accept poll and
the explicit `set_nonblocking(false)` restoration. See
`docs/CAPY_CAMERA_001C22_REPORT.md`. The corrected V2419A run observed Camera
Service IDs `0 → 1 → 0`, three distinct canonical CAVC configs and one
successful 60-second `CapyIO Camera` hold with bounded receiver/mapping cleanup.

CAPY-CAMERA-001C23 adds hardware-free endpoint and orchestration coverage. The
Java contract proves blank input preserves ADB reverse, returns defensive
address copies, accepts RFC1918/link-local/100.64.0.0/10 literals and rejects
DNS, public, loopback, multicast, IPv6, host-with-port and non-canonical input.
Rust tests require paired trusted-LAN bind/peer options, a different exact peer,
fixed port 38173 and exact IPv4 peer admission. The Windows command test pins
the two-argument `trusted-lan-live-hold <bind-ipv4> <phone-ipv4>` surface and
the fixed receiver arguments. Structural validation rejects a wildcard
listener or name-resolving Android destination.

A separately authorized physical C23 run used no ADB reverse mapping and
accepted 358 access units, including 12 keyframes. All 358 units decoded and
were published through the Global mapping; distinct sampled checksums proved
changing NV12 pixels, and one Session/CurrentUser `CapyIO Camera` enumerated.
That run did not capture ordinary Windows Camera pixels and the external
consumer retained the mapping beyond the short hold, so it is transport/
decode/publication/enumeration evidence rather than a completed end-user and
cleanup regression. See `docs/CAPY_CAMERA_001C23_REPORT.md`.

CAPY-CAMERA-001C24 adds focused regressions for the timing and cleanup defects
exposed by C23. The Android contract pins 120 fixed connection attempts with
the existing 500 ms connect/retry bounds. Receiver tests accept a reconnect at
exactly 60 seconds and reject 60,001 ms. Windows tests pin a 120-second initial
mapping wait, the fixed 60-second receiver grace and cleanup-error precedence:
receiver or mapping cleanup failure must surface as `stage=cleanup` instead of
being hidden by an earlier validation error. Structural validation also pins
foreground keep-screen-on behavior and truthful Camera2-ID/vendor-Zoom labels.
The C24 physical run below exposed the remaining rotation defect; C25 provides
the closing exact-hash regression.

The authorized C24 physical run installed the exact APK, used no camera
`tcp:38173` reverse, reached one trusted-LAN config, a validated Global mapping
and one Session/CurrentUser `CapyIO Camera`, and then completed receiver/mapping
cleanup. A non-elevated attempt first proved that Global mapping creation
requires the approved elevated lab context (`CreateFileMappingW` error 5).
Ordinary Windows Camera pixels were not captured because phone rotation
recreated the Activity and stopped the foreground session. ProgramData, CLSID,
port and process cleanup subsequently passed; an unrelated `tcp:61000` reverse
was preserved.

CAPY-CAMERA-001C25 structurally requires the Activity to handle orientation,
screen-size and smallest-screen-size changes and forbids hiding the defect with
an orientation lock. Source validation requires an explicit
`onConfigurationChanged` handler while retaining the ordinary `HOST_PAUSED`
close path. Offline contract/build/lint and packaged-manifest/permission checks
pass. The authorized exact-hash V2419A regression retained Activity record
`57893458` and stream/epoch
`89f71f50b95af3e0ba79e69574e5b419/13862327667183` across
portrait/landscape/portrait. One Session/CurrentUser camera appeared and
ordinary Windows Camera showed two visibly different live frames 1.2 seconds
apart. Moving the Activity behind another app ended the CAMERA app-op duration,
recorded a CameraService disconnect and left no active camera client. No camera
reverse was created, the unrelated `tcp:61000` mapping remained, and final
Windows process/port/ProgramData/CLSID preflight was clean. See
`docs/CAPY_CAMERA_001C25_REPORT.md` for the exact artifact and evidence limits.

CAPY-CAMERA-001C26 adds a hardware-free late-producer regression. The registered
provider still uses a valid mapping immediately, but exact mapping absence now
selects an asynchronous placeholder provider rather than a permanent fixture.
Unit tests start that provider before a unique Local test mapping exists, emit
the fixed placeholder interval, create and publish through the mapping later,
and assert one continuous virtual stream/epoch/sequence/timestamp with a
discontinuous first live frame. A subsequent empty live read returns
`MF_E_NOTACCEPTING` to the existing bounded async retry instead of emitting
another fixture. An invalid test mapping target fails closed. Normal tests use
only the feature-gated Local namespace and make no registry, Global mapping,
virtual-camera API, APK or device change. Ordinary Windows Camera late-attach
evidence requires a separately authorized hash-recorded package.

CAPY-CAMERA-001C27 adds hardware-free producer-stall and reattachment
regressions. The fixture suite starts a deterministic source at an explicit
non-zero sequence/timestamp and verifies checked continuation plus sequence and
timestamp overflow rejection. The registered-provider tests begin directly on
a published Local mapping, retain `MF_E_NOTACCEPTING` for the first 399 empty
live polls, and require the 400th to emit a discontinuous placeholder at the
next output sequence/timestamp. One case drops the producer, proves the old read
handle was released by creating a replacement producer under the same name,
then observes a discontinuous live reattachment after the fixed probe interval.
A second keeps the producer alive, proves the reopened last publication remains
placeholder rather than replaying stale luma, publishes one newer frame and
then observes reattachment. These tests do not use the registry, Global mapping,
virtual-camera API, APK, ADB or a physical device. Exact ordinary Windows Camera
placeholder/live/fallback/restart evidence remains separately authorized.

## Windows synthetic touchpad API probe

- parameter tests enforce a positive physical himetric size and `1..=5`
  contacts before any platform call;
- the default command dynamically checks the four required `user32.dll`
  exports and performs no device creation or input injection;
- the explicit create-device smoke creates and immediately destroys one
  five-contact synthetic touchpad without submitting a contact frame;
- absent exports and creation failure remain typed probe outcomes rather than
  being mislabeled as a working Projection;
- 1/2/3/4-finger behavior, Windows Settings integration, RDP/multi-user
  behavior and process-loss cleanup require a later explicit lab fixture.

The probe installs no driver and makes no VHF, HID enumeration or Android
end-to-end claim.

## Touchpad platform mapping tests

- Android mapping requires an initial epoch cancellation and maps down, move,
  pointer-down, pointer-up, up and cancel to post-event complete snapshots;
- Android tests cover stable/reordered IDs, himetric coordinate scaling,
  calibrated pressure clamping, invalid action indexes, non-finger tools,
  duplicate IDs, surface/contact bounds and transactional timestamp failures;
- Android capture-session tests cover start barriers, reordered multi-touch,
  cross-event pointer-ID drift, transactional mapping failure, stop/restart,
  timestamp failure during cleanup, idempotent stop/close and closed-state
  rejection;
- Windows projection tests cover active/released/cancelled records, immediate
  cleanup after gaps and epoch changes, suppression until cancellation and a
  five-for-five replacement split into two batches bounded at five each;
- the Windows native encoder test checks PT_TOUCHPAD, documented pointer flags,
  himetric predicted/raw coordinates, zero cross-clock timestamps and omitted
  pixel-defined optional contact fields.

These tests inspect DTOs/native structures and the pure Rust capture boundary
only. They do not call `InjectSyntheticPointerInput`, create an Android OS/JNI
runtime, install an APK or
prove a physical cursor, tap, pan/zoom or global three/four-finger gesture.

## Controlled touchpad injection harness tests

- fixture tests validate four closed one-shot gestures with eight active
  updates between initial cancellation and final release/cancellation;
- exact dry-run metrics cover 1/2/3/4 contacts, 11 projected frames, nine
  native batches and maximum one batch per fixture frame;
- CLI parser tests prove dry-run is the default, unknown gestures/arguments are
  rejected, and injection requires a separate desktop-impact acknowledgement;
- invalid device parameters fail before user32 loading or device creation;
- all ordinary commands print `device_creation=not_requested` and
  `input_injected=false`.

CI never supplies the two injection gates. API submission success and observed
Windows behavior require a separately approved controlled-lab run.

## Synthetic touchpad Sink session and host acceptance

- fake-device unit tests cover explicit close, epoch cancellation, abandoned-
  session drop, transactional projection rejection, primary submission failure
  and retained cleanup failure;
- a failed or closed session rejects further frames, while a pure projection
  error leaves the session active and the rejected sequence reusable;
- ordinary session tests never load user32 or submit desktop input;
- on Windows build 26200.9168, the approved fixed one-finger command first
  failed with Win32 error 5 under the isolated `CodexSandboxOffline` account;
- the same already-built fixed binary then succeeded in the authorized
  interactive host context, reporting nine submitted batches and nine contact
  records.

The successful API return establishes accepted one-finger native submission,
not independently observed cursor displacement. Later composed exact-name
tests established accepted two-, three- and four-finger native submission; the
user separately confirmed visible scrolling for the two-finger run. Visible
three-/four-finger behavior, settings policy and remote
end-to-end behavior remain separate controlled acceptance work.

## Private remote-touchpad packet tests

- exact header-only cancel and five-contact 152-byte maximum packets round-trip
  deterministically;
- encoder rejects wrong Stream, stale/future epoch and semantic contract
  violations before producing bytes;
- decoder rejects short/long/non-exact packets, wrong magic/version/enums,
  excess contacts, reserved data, unknown flags and non-canonical optional
  fields;
- explicit epoch advance rejects old packets and non-advancing transitions;
- duplicate decoded contact IDs still pass through and fail the ordinary
  touchpad semantic validator;
- a hardware-free boundary test maps Android cancel/down/two-finger/move/up
  snapshots through packet encode/decode into Windows active/release batches.
- packet-Source tests pass a start/down/two-finger/reordered-move/up/stop
  lifecycle from the Android capture session through seven encoded and decoded
  packets; they also cover initial-cancel enforcement, transactional sequence
  gaps, active-contact close refusal, idempotent close and terminal rejection.
- delivery-session tests cover successful commit/release, exact same-frame retry
  after definite pre-write rejection, fail-closed unknown delivery, admission
  revocation, binding drift, construction mismatch and exactly-once channel
  cleanup including abandoned-session Drop.
- sender Runtime-worker tests use an actual `NodeRuntime` Route and prove that
  construction, each frame and close independently sample the trusted clock and
  fetch current Route state; authorization expiry, Route stop, inactive initial
  state, provider/clock failure and clock rollback all close the channel without
  delivering an unadmitted packet.
- concrete host-channel tests cover invalid capacity, full-queue definite
  rejection, ordered drain after sender close, admission denial, binding
  replacement, stale-packet discard, receiver close and idempotent cleanup.

These tests open no socket, create no synthetic device and inject no desktop
input. Authentication, encryption and transport reconnect remain future
acceptance work.

## Private remote-touchpad receiver tests

- invalid packet-rate and active-idle bounds fail during construction;
- duplicate/late packets, malformed packets and trusted-local arrival-clock
  regression close the Sink, poison the receiver and do not submit a rejected
  frame;
- a sequence gap remains observable and makes the hardware-free Windows
  projector emit cancellation for its retained contact;
- the fixed one-second rate window rejects the first excess packet and resets
  only after a valid explicit epoch advance;
- active contacts close at the exact idle deadline while an already released
  stream remains idle;
- Sink submission failure retains both the primary and failed-cleanup error;
- explicit disconnect and abandoned active receiver drop each close once.

All receiver tests use a pure in-memory Sink or `WindowsTouchpadProjector`.
They do not construct `SyntheticTouchpadSession`, call user32, open a socket or
inject desktop input.

## Private remote-touchpad Route ingress tests

- construction rejects zero/oversized queues, wrong backend/Profile/Sink,
  Pending or expired authorization, non-live Route state and stream/Route epoch
  mismatch;
- a Starting binding rejects packets until explicit activation against the
  same current Active Route;
- ordered enqueue/pump preserves sequences through the receiver and reports a
  bounded pump result;
- queue overflow clears pending records and closes without submitting them;
- Route Offline, authorization expiry and unexpected epoch change each close
  the affected session;
- oversized packets, local-clock regression and records left queued through
  the active-idle deadline never reach the Sink;
- an empty pump closes active contacts at the exact idle deadline;
- a strictly later Starting Route epoch discards old queued records, advances
  the Sink and accepts only the fresh epoch after reactivation;
- cleanup failure remains attached to the primary Route-session fault;
- the hardware-free Core Route → fixed queue → receiver → Windows projector
  path applies cancel, active and release snapshots without a native device.

Ten Route-ingress tests use only deterministic Core values, local timestamps,
memory and `WindowsTouchpadProjector`. No Runtime thread, socket,
`SyntheticTouchpadSession`, user32 call or desktop input exists in the test
path.

## Private remote-touchpad Runtime worker tests

- a dev-only Route provider reads owned snapshots from a real `NodeRuntime` as
  its Route moves Starting to Active to Stopping/Stopped;
- one worker drives activation, packet enqueue, periodic pump and explicit stop
  without accepting Route snapshots or timestamps from the packet caller;
- a later Runtime Route epoch discards the old queue, advances the memory Sink
  and accepts only the reactivated epoch;
- millisecond clock rollback fails closed even while nanoseconds advance; and
- Route-provider failure, clock failure and observed Runtime stopping state each
  close and poison the session.

Five receiver worker tests use the existing local `capyio-runtime` and `capyio-testkit`
crates only as development dependencies. They create no thread, timer, socket,
`SyntheticTouchpadSession`, user32 call or desktop input.

Four sender delivery-worker tests reuse that real Runtime provider boundary.
They verify three admitted frames plus close against five separate Route/clock
observations, and cover authorization expiry, Runtime stop, inactive
construction, provider/clock failure and rollback. Fake admitted channels retain
packet and exactly-once-close evidence; no transport or platform device exists.

One additional composition test connects the Runtime sender worker to the
Runtime receiver worker through the concrete four-packet host channel. Initial
cancel, one active contact and release reach `WindowsTouchpadProjector` in
order, followed by bounded receiver cancellation on close. The test uses a real
local `NodeRuntime` Route but no socket, native device or desktop input.

A separately ignored Windows acceptance uses the identical sender/channel/
receiver composition with `WindowsSyntheticTouchpadSinkFactory`. After explicit
approval it submits exactly four fixed one-finger packets, verifies all four
were enqueued and processed, releases contacts and closes the synthetic device.
Native API success is recorded separately from visible pointer observation.

Four private stream-record tests cover exact full-binding Hello with and without
authorization expiry, maximum five-contact Data at 176 bytes, exact Ack/Close,
outer/embedded epoch and sequence agreement, and rejection of truncation,
unknown flags, binding mutation and invalid embedded packet length. They perform
no I/O and make no authentication claim.

Three additional construction-boundary tests prove that a stopped Route or
invalid touchpad descriptor opens zero Sinks, a valid preflight opens exactly
one memory Sink, factory failure is typed, and the Windows production factory
satisfies the interface without calling `open`. The eight worker tests remain
device-free; the compile-time Windows factory assertion does not create a
synthetic device.

One ninth Windows-only worker test is ignored by default. With explicit human
approval it constructs a real `NodeRuntime` Route, passes authorized preflight,
opens and immediately closes `SyntheticTouchpadSession`, and asserts zero
enqueued/processed packets. The exact test submits no contact frame and is not
part of `cargo xtask ci`.

A tenth Windows-only worker test is also ignored by default. After separate
approval it activates the real Route and submits the compiled
CancelAll/down/move/release lifecycle through private packet encode, enqueue,
pump and `SyntheticTouchpadSession`. It verifies four packets enqueued and four
processed before release/close. A passing API result is retained separately
from any claim of visually observed pointer behavior.

An eleventh Windows-only worker test is ignored by default and requires a
separate approval. It submits CancelAll, two contacts down, synchronous vertical
translation and empty release through the real composed path. Four packets are
enqueued/processed before close. Passing native submission is not treated as
proof that the foreground application visibly scrolled.

## Android touchpad JNI and AAR tests

`cargo test -p capyio-android-jni` exercises the composition boundary without a
JVM or phone. It proves contiguous cancel/down/move/up packets, preserves both
contacts through a two-finger lifecycle and rejects unequal primitive arrays
before capture state mutates.

The Android target check uses `cargo ndk` with NDK 29 to link the release
`arm64-v8a` shared library. The separate Gradle release build compiles real
`MotionEvent` Kotlin code and packages the shared library into an AAR. Neither
targeted build installs an APK, opens a socket, creates a Windows synthetic
device or changes Android permissions. Wireless ADB ABI/API queries are
read-only evidence and are not required by hosted CI.

## Live Android-to-Windows touchpad lab

The CAPY-PTP-002V procedure requires explicit approval for the Internet
permission, APK installation and desktop input. The APK permission inventory
and installed hash are verified before execution. The Windows binary listens
only on `127.0.0.1:61000` and requires both `--inject` and
`--acknowledge-desktop-input`; it rejects mismatched Hello before device
creation. ADB reverse supplies the paired lab tunnel. Every Data record receives
an Ack only after native submission.

The accepted run processed initial cancellation plus an Android-generated
down/move/release lifecycle as 44 frames and 43 native batches/contact records.
The receiver closed the virtual device. A mixed Android clock-domain attempt
and a sandboxed Windows submission attempt failed closed and did not count as
acceptance. Hosted CI never installs the APK, creates the ADB mapping or submits
desktop input.

CAPY-PTP-003O repeats the same exact binding over the installed VHF fallback.
The receiver must add explicit `--vhf`, run elevated because of the protected
Broker ACL, and still validate Hello before opening the interface. Its first
physical phone gesture produced 80 acknowledged VHF frames with contact
transitions `0 -> 1 -> 0`, then closed cleanly. The separate `003P` acceptance
processed 830 physical frames, reached `0 -> 1 -> 2 -> 3 -> 4`, submitted all
accepted frames to VHF before acknowledgement and closed normally. Its bounded
wrapper passed with `max_contacts_observed=4`. This is physical transport/device
evidence; the remaining live Shell observation is distinct from the already
passing fixed Windows-generated three-/four-finger fixtures.

CAPY-PTP-003Q moves the lab's Hello/Data/Ack/Close ordering into the reusable
`PrivateTouchpadTransportReceiver`. Five hardware-free tests prove that invalid
construction or mismatched Hello opens no Sink, valid active/release Data is
acknowledged only after submission, malformed Data closes without submission,
and factory failure/active timeout are terminal. The existing lab argument test
also proves VHF remains explicit opt-in. Default tests open no device or socket.

CAPY-PTP-002W separates local touch capture from transport readiness. Before an
initial connection, touch must remain visibly consumed by the Activity while no
packet sequence is advanced; the sender retries the loopback endpoint. The live
vivo launch must reach `hello_binding=accepted` and `device_creation=created`.
Manual physical one- through four-finger behavior remains separate evidence.
Immersive bars, the Android system-gesture exclusion region and parent
interception denial are tested as app-owned best effort and must not be reported
as suppressing OEM gestures that are intercepted before `MotionEvent` delivery.
The manual receiver mode is separately selected with `--manual-session` and
raises only the bounded idle deadline from 30 seconds to 600 seconds. Sustained
phone input must not exceed four pending MOVE records; lifecycle records are
never sampled away.

CAPY-PTP-003T adds deterministic Android multi-finger motion-policy tests. They
prove one-/two-finger identity mapping, no position jump when the third contact
arms attenuation, 700-per-mille motion while contacts remain, identity reset
after complete release and rejection of scales outside `1..=1000`. Pointer
lifecycle remained separate from the Activity's then-current 24 ms initial/72 ms
added-contact MOVE settling. The arm64 JNI build, v0.7 debug APK and Android lint pass; the
installed APK hash matches the inspected local package and its manifest still
declares only `INTERNET`. The tuned live run processed and submitted all 2,165
frames, reached four contacts and closed cleanly.

User comparison found easier gesture effects but unreliable rapid simultaneous
three-/four-finger placement. Timestamped Activity logs repeatedly show
pointer-down through three or four contacts followed within 1--3 ms by
`ACTION_CANCEL`; read-only input diagnostics identify a full-screen system-UID
`global_gesture_monitor`. Raw input sampling separately observes tracking-ID
loss under light contact. CAPY-PTP-003U therefore counts probable system
interceptions and offers a user-initiated vivo SmartShot settings route. The
v0.8 debug APK builds and lints, declares only `INTERNET`, resolves the intended
settings Activity on the identified device and has a matching installed hash.
The post-setting physical comparison processed and submitted all 2,050 frames,
repeatedly reached four contacts and closed cleanly after the user disabled the
conflicting OEM gestures. The setting decision remains manual because ordinary
app tests cannot synthesize or suppress an OEM global gesture monitor.

CAPY-PTP-0041 Android v1.6 removes the initial 24 ms MOVE settling only for a
new one-finger `ACTION_DOWN`, resets the 16 ms sampling epoch at that boundary
and retains the existing 72 ms added-contact settling for multi-finger
stability. A pure tap/drag feedback tracker test covers an immediate second
contact, single-fire movement threshold, timeout, distance, moved-first-contact
and multi-touch cancellation cases. The app emits one setting-respecting local
`DRAG_START` haptic when the second contact crosses Android touch slop; it adds
no permission and does not claim that Windows accepted the semantic drag.
Android v1.7 adds the explicitly authorized `VIBRATE` permission and a
persistent, default-enabled in-app switch that uses predefined vibrator effects
independently of the global touch-feedback setting. Android v1.8 extends the
local feedback contract so every qualified one-finger tap emits a light `TICK`
(two taps therefore emit two ticks), while drag lock continues to emit the
stronger `CLICK`. The pure tracker test covers both tap events in a double tap.
Android v1.9 persists a weak, medium or strong intensity and applies it to both
tap and drag effects, preferring supported vibrator primitives and retaining a
bounded amplitude fallback. Its pure strength tests cover cycling, invalid
preference fallback and bounded per-effect scales/amplitudes. The VHF lab Sink
also gains a bounded ClickPad Button 1 compatibility latch: after a qualified
first tap, a nearby second one-finger down within 500 ms is pressed from its
first frame and remains pressed through movement until release. Receiver tests
cover press/hold/release plus far-contact and multi-touch rejection. This latch
does not change the private Android packet stream, user32 synthetic Sink or
installed driver package; physical file-drag comparison remains required.
Android v1.10 adds UI-only physical diagnostics before pressure-driven behavior
is considered. A pure observer accumulates finite non-negative per-contact
pressure and touch-major/minor values, counts five-contact frames, and counts at
most one five-contact reach per complete gesture. Duplicate IDs and invalid
values are rejected transactionally; the Activity ignores a rejected diagnostic
sample while continuing the unchanged JNI/network input path. Three pure tests,
debug assembly and lint pass. Pressure, geometry and Mechanical Force are not
yet submitted to VHF, and no five-finger Windows action is claimed.
The physical v1.10 comparison then recorded 9,074 samples with pressure fixed at
`1.00..=1.00`, maximum contact axes `2.40 x 1.60`, 828 five-contact frames and
11 complete gestures reaching five contacts; the unchanged receiver
acknowledged 2,530 frames and Android reached `maxContactCount=5`. This proves
five-contact capture, not a Windows five-finger action. It also rejects
pressure-driven behavior for the identified vivo because its ordinary-finger
pressure signal has no observed dynamic range.

CAPY-PTP-003V adds a VHF-only cursor-observation gate. It requires an exact
one-contact release with at least 100 himetric of source displacement, ignores
two-contact gestures and taps, anchors the interactive Windows cursor once and
reports both source and host deltas. Four CLI/motion-gate tests are hardware
free. The accepted physical run submitted all 155 frames and moved the cursor
from `(960,600)` to `(794,239)`. An unchanged sample taken while CapyIO was not
the resumed phone Activity is invalid for the user's actual separate-computer
UU topology and is not used as causal evidence. Version 1.0 still correctly
closes on foreground loss. CAPY-PTP-003Z uses the existing bounded 600-second
manual mode for continuous lab use instead of a one-shot release gate. Its
accepted Android 1.2 run submitted all 4,356 frames, reached four contacts and
closed cleanly only after Android Close. The user confirmed all requested
interactions except double-tap followed by drag; a read-only Windows query
reported that tap-and-drag is enabled, so that input shape remains a separate
diagnostic rather than a persistent-session failure.

## VIIPER Xbox 360 codec tests

- a fixed-revision golden vector distinguishes the external device-stream
  `InputState` frame from VIIPER's different same-length host USB report;
- every representable semantic button and all nine D-pad combinations map to
  exact little-endian masks;
- six independent source-axis sign selectors, asymmetric signed endpoints and
  trigger rounding are explicit;
- current and future source buttons without Xbox 360 fields fail closed, while
  all six reserved bytes remain zero;
- rumble feedback accepts exactly two bytes, preserves left/right raw motor
  intensity and rejects both truncated and trailing data.

These tests do not open a socket, start VIIPER, attach USB/IP, install a driver,
enumerate a system controller, prove game compatibility or construct a haptics
command without a duration/Route lifecycle.

## VIIPER DualShock 4 codec tests

- the pinned 31-byte stream layout keeps controls, two inactive touch contacts,
  three fixed-point gyro axes and three fixed-point acceleration axes at exact
  offsets;
- semantic face buttons use DS4 physical positions, non-zero analog triggers
  set their matching digital bits, and signed sticks cover both asymmetric
  `i8` endpoints;
- separately typed SI-unit IMU envelopes map through explicit signed axis
  permutations to 16 counts per degree/second and 512 counts per m/s²;
- the fixed Controller Lab landscape mount maps Android source `+Y,+Z,+X`
  into DS4 body X,Y,Z while leaving the StandardPort sample in its normative
  Android device coordinate frame;
- duplicate axes, invalid envelopes, fixed-point overflow and unsupported
  paddles fail closed before a packet is emitted;
- feedback accepts exactly seven bytes and preserves both rumble motors, RGB
  LED and flash timing bytes without inventing a reverse haptics Route.

These tests do not provision `dualshock4`, connect to a live VIIPER stream,
attach USB/IP or prove that a Windows game consumes native DS4 motion.

## VIIPER owned DualShock 4 session fixture tests

- a loopback fixture observes exact probe, create, fixed `dualshock4` add,
  device stream, initial neutral/stationary state and owned-bus removal;
- the response must match DS4 `054c:09cc`, a numeric device ID and object-shaped
  fixed metadata; an identity mismatch rolls back the known bus;
- gamepad and IMU streams have independent sequence trackers. A gamepad-only
  gap releases controls while retaining the last accepted motion sample before
  the recovered report;
- exact seven-byte feedback is read without creating a reverse Route, and
  explicit stop writes the safe state before stream shutdown and cleanup.

The USB/IP unit boundary separately filters DS4 exports and verifies an owned
port against the exact server, VIIPER bus and `054c:09cc` identity. Desktop
tests prove motion-only DSU never submits local controls. No test performs a
real attachment.

## Runtime-owned DualShock 4 paired Route fixture tests

- controls and IMU remain two independent StandardPort Routes with their own
  profiles, QoS modes, Route IDs and epochs;
- the shared DS4 Worker activates only after both anchors match their Runtime
  epochs and exact VIIPER provisioning succeeds;
- an IMU-source disconnect writes the safe state, removes the owned bus,
  offlines both dependent DS4 Routes and leaves an unrelated active Route
  unchanged;
- retry recovers both Routes with advancing epochs and a fresh owned bus;
- mismatched anchors fail closed before any VIIPER connection is opened;
- the optional USB/IP branch is constrained to the separately tested one-shot
  DS4 selector and exact owned-port lifecycle.

These fixtures use a process-local fake VIIPER server. They do not attach
USB/IP, enumerate a Windows DS4, install a driver or connect to a phone.

## Desktop DS4 selection and complete-state ingress tests

- the Windows host DTO keeps Xbox 360 and DS4 identities explicit and defaults
  the motion-capable path to DS4 `054c:09cc`;
- read-only preflight selects the controller-specific USB/IP inventory method,
  so DS4 mode cannot accidentally accept an Xbox export or vice versa;
- one accepted Android datagram remains one paired controls+IMU event on a
  capacity-eight non-blocking channel;
- peer timeout and listener stop emit explicit upstream-offline events rather
  than performing blocking projection cleanup on the UDP receiver thread;
- the desktop inspector drains the bounded ingress and publishes accepted
  packet, offline-event and last-remote-sequence counters;
- starting DSU replaces the observer ingress with the DSU senders, and stopping
  DSU restores the bounded Windows ingress.

These tests do not start the DS4 Worker, attach USB/IP or mutate Windows. The
ingress-to-DS4 activation Worker remains the final hardware-free 004C slice.

## VIIPER bounded probe fixture tests

- configuration rejects non-loopback IPs, port zero, zero/overlong deadlines
  and response limits outside `1..=4096`;
- a process-local `127.0.0.1:0` fixture observes exact `ping\0`, and the client
  requires exact `VIIPER` / `0.7.0` identity;
- the response limit accepts exactly N bytes and rejects N+1 before JSON parse;
- whitespace-only, malformed/trailing JSON, fixed Problem responses and wrong
  identity/version have distinct failures;
- a complete JSON prefix held open without EOF times out rather than being
  accepted early.

The fixture uses bounded channel joins and socket deadlines. It never starts or
connects to a real VIIPER server, creates a bus/device, enters a device stream,
attaches USB/IP or touches a driver/certificate store.

## VIIPER owned Xbox 360 session fixture tests

- a process-local server observes exact `ping`, create, fixed Xbox add, stream
  handshake, initial neutral and remove ordering;
- unsupported controls and rejected stream headers do not consume sequence or
  write a frame;
- an input gap writes neutral before the recovered state, epoch advance writes
  neutral before fresh input, and sequence exhaustion writes neutral and
  latches until explicit epoch advance;
- stop writes neutral, shuts down the device stream, removes only the owned bus
  and is idempotent;
- consecutive exact rumble frames, a no-byte timeout, one-byte truncation and
  clean peer close are distinguished; failed streams still attempt bus cleanup;
- add failure and injected device identity trigger removal of the known bus,
  and a cleanup failure is retained alongside the primary failure.

Expected stream packets are hard-coded independently of the production codec.
All tests use `TcpListener(127.0.0.1:0)` and a fake protocol peer. They neither
start VIIPER nor create a real virtual device or USB/IP attachment.

## VIIPER Runtime Route fixture tests

- a typed remote gamepad Source and local VIIPER Sink form one
  `ExternalProtocol` Route, which reaches Active only after the owned Worker
  sends initial neutral;
- unsupported controls reject one frame without consuming sequence or taking
  the Route Offline;
- upstream disconnect sends neutral and removes the owned bus before Offline,
  while retry provisions a fresh bus under a strictly newer Runtime epoch;
- add failure rolls back the known bus before the typed open-failure Problem;
- sequence exhaustion sends fail-safe neutral, cleans up and requires an
  explicit Route retry;
- a closed device stream is detected by bounded raw-feedback polling, cleaned
  up and reported as a stream Problem without constructing a haptics Route;
- a simultaneously Active IMU Route remains Active with no related Problem in
  every injected gamepad failure case.

The four tests use only process-local `127.0.0.1:0` listeners and independently
hard-coded request/frame expectations. They do not start or configure VIIPER,
touch USB/IP or a driver, enumerate a controller or make a game-compatibility
claim.

## DSU Runtime Route and live-lab fixture tests

- the DSU composition registers a typed IMU StandardPort sink and does not
  activate until its Worker binds IPv4 loopback with the exact Runtime epoch;
- upstream disconnect joins the Worker and releases its port before Offline;
  retry binds a new Worker under a strictly newer epoch;
- occupied-port and mismatched-epoch starts produce owner-correct typed
  Problems while a simultaneously Active unrelated IMU Route remains unchanged;
- the lab CLI parser closes commands, ports, sample count and signed axis
  permutations, while preflight releases the exact checked UDP port;
- a process-local two-WebSocket SensorServer fixture emits accelerometer and
  gyroscope readings through the real assembler, Runtime Route, DSU Worker and
  a real loopback DSU subscriber; acceptance requires 16/16 samples, observed
  subscription/motion delivery and zero queue, projection or transport errors.

All automated endpoints are ephemeral loopback fixtures. The command does not
prove a phone, emulator UI, game response or physical orientation. Those are
retained only by the manual procedure in `docs/CAPY_GAMEPAD_DSU_LAB.md`.

## Desktop Controller source and inspector tests

- a closed semantic Tauri DTO feeds the portable `GamepadStateComposer`; a
  button followed by a stick update retains both controls in complete snapshots
  and monotonically advances sequence;
- invalid D-pad and reserved signed-axis inputs fail without consuming sequence;
- reset returns buttons, D-pad, sticks and triggers to neutral;
- the desktop simulator starts the existing bounded DSU dual-input Worker on an
  ephemeral IPv4-loopback fixture, seeds a labeled stationary motion sample,
  submits one complete control state and observes it accepted with no queue
  overflow before bounded neutral stop;
- browser typecheck/build and visual checks cover the desktop layout, top-level
  Controller navigation, button press/release sequencing, 844x390 responsive
  layout, absence of horizontal overflow and absence of browser errors.
- the Windows status preflight has no input DTO, filters the bounded USB/IP
  inventory to exact Xbox 360 exports, distinguishes zero/one/multiple matches,
  preserves partial VIIPER/USB-IP readiness and maps host failures to stable
  sanitized codes without producing an owned attachment; a single-flight test
  rejects concurrent invocation and proves permit release;
- a real pre-restart run covers service-offline, ready-without-export,
  unique-export-ready and post-cleanup disappearance states while the owned
  USB/IP port remains empty. See `CAPY_GAMEPAD_006C_REPORT.md`.

Those `005A` checks do not package or install an Android application, capture a
phone sensor, establish a peer data plane, observe an emulator subscription or
prove game compatibility. The `005A` DSU source and motion seed are explicitly
simulated.

## Android Controller Lab tests

- closed UDP decoding accepts a valid complete control+motion frame and rejects
  unknown fields, a wrong token and the reserved `i16::MIN` axis value;
- a real loopback UDP listener accepts one sequence, rejects its replay and
  replaces a pressed control with neutral after the 350 ms peer deadline;
- the composition acceptance starts Android input and the existing DSU Worker,
  sends a complete UDP frame, observes the desktop source change to
  `android_touch`, verifies button/stick/motion values and observes the DSU
  controls queue accept the same state;
- Android `lintDebug` and `assembleDebug` compile the native Java surface for
  minSdk 26 / targetSdk 36 without a third-party runtime dependency; `aapt2`
  verifies package `io.capyio.controllerlab` and exactly one declared permission,
  `android.permission.INTERNET`;
- physical checks install only the recorded debug APK on the explicitly
  authorized device, launch the exported launcher Activity, inspect the visible
  UI, send touch+IMU frames to the desktop and verify timeout neutralization;
- the debug-only `--gamepad-physical-gate` registers a DSUC subscriber, waits
  for real Android data, closes it, then requires a replacement subscriber to
  observe a non-neutral control and finite IMU values. This covers Windows UDP
  `ConnectionReset` recovery after a stale subscriber as well as the complete
  phone-to-DSUS packet path;
- the debug-only `--gamepad-viiper-physical-gate` composes that same accepted
  Android stream into DSU and a real pinned VIIPER v0.7.0 Xbox 360 session. It
  prints exact bus/device IDs, stays alive for a bounded operator-selected
  enumeration window, neutralizes peer timeout, and explicitly removes only
  its owned bus. Windows USB/IP installation and attachment are separately
  authorized host-lab operations recorded in `CAPY_GAMEPAD_006A_REPORT.md`;
- `capyio-gamepad-usbip-lab preflight` executes exact-version probe and
  read-only list against the live export. Fixtures reject non-loopback/unbounded
  config, malformed or wrong-VID exports, persistent/all-device arguments,
  ambiguous ports and oversized output. A fake executor proves ordered
  probe/list/one-shot-attach/exact-port-detach without touching a driver. The
  real pre-restart run stops after list; see `CAPY_GAMEPAD_006B_REPORT.md`.
- post-restart fixtures require the terse returned port to resolve through
  `usbip port <port>` to the exact loopback server, VIIPER bus and Xbox
  `045e:028e` identity. Missing/mismatched inventory triggers exact-port
  cleanup; the physical lab polls that same invariant once per second and the
  recorded `006D` run proves PnP, XInput button/axis delivery, liveness and
  exact detach.
- the `--gamepad-windows-ds4-only-runtime-gate` keeps the optional XInput
  companion disabled and requires physical Android packets plus non-neutral
  controls before exact cleanup. The recorded 150-second run accepted 7,096
  packets and 723 non-neutral states; the independent WGI probe observed an
  advancing `054c:09cc` report with button, D-pad switch and axis changes.
- a paired Android projection replaces each transient 350 ms WLAN peer gap
  with a complete neutral controls+motion state while keeping the virtual
  controller owned and active. Explicit listener stop remains terminal and
  removes the projection.
- the fixture DSU subscriber uses bounded nonblocking 1 ms polling so its
  initial request cannot be lost behind a 100 ms receive wait before the
  Runtime-owned loopback endpoint binds.

Loopback automation proves the wire/parser/composition boundary but not Android
touch dispatch, physical SensorManager axes, WLAN/firewall behavior, emulator
response or game compatibility. The recorded `005B` run proves the first four
physical boundaries through a conforming DSU subscriber; emulator/game behavior
still requires a separately installed Cemu/Dolphin instance.

## SensorServer mapping contract tests

- the pinned upstream three-field JSON shape maps exact finite axes, timestamp
  and Android accuracy values;
- empty, oversized, malformed, unknown-field, wrong-axis-count, zero-timestamp
  and unknown-accuracy messages fail explicitly;
- accelerometer and gyroscope readings pair in either arrival order only inside
  a configured skew bound;
- each required reading is consumed once; replacing an unpaired sample is
  observable and a later in-skew sample recovers;
- timestamp regression and sequence exhaustion fail closed;
- optional fresh magnetic-field data and every component timestamp remain in
  the IMU Profile output.

These tests use recorded synthetic JSON and no WebSocket implementation, phone,
APK or network connection.

## SensorServer WebSocket contract tests

- endpoint construction accepts only typed IP addresses, non-zero ports and
  fixed per-sensor paths;
- a loopback RFC 6455 server proves exact text-message mapping;
- ping/pong, close code and socket timeout have distinct outcomes;
- malformed JSON, binary data and messages above 4 KiB do not reach the IMU
  consumer;
- an HTTP upgrade response exceeding Tungstenite's 64 KiB handshake attack
  limit fails the connection;
- dependency validation pins Tungstenite 0.30.0 to `handshake` only and rejects
  async/TLS additions in this slice.

These loopback tests open only an ephemeral local port. They do not connect to
the phone, install an APK or claim production authentication.

## Deterministic integration tests

Fixtures use HP OmniBook Ultra Flip 14 and vivo X200 Pro mini with no environment
or hardware reads. Tests register both catalogs, open a Session, prepare/start
opposite-direction Routes, stop one, simulate Adapter/peer loss and assert
ordered bounded events/snapshots.

The schema-4 UI catalog fixture additionally includes Android and Windows
Precision Touchpad capabilities. Tauri tests prove that only a Capability on
the local Node with a Source Port can be selected for sharing; peer Sources and
local Sink-only projections fail closed. Browser Mock and Tauri return the same
typed capability, availability, permission and Port-direction fields, and the
UI build typechecks both local and peer catalog components. These tests prove
catalog projection and selection policy only, not physical hardware discovery,
peer authentication, permission grant or data publication.

## Audio Share external-process probe tests

- configuration requires an explicit IP address, non-zero port, bounded
  enumerated endpoint ID, encoding, channel count and sample rate;
- server arguments are direct process arguments and never a shell string;
- pinned version and endpoint-list parsing enforce output, line, count, ID and
  name bounds, reject duplicates/mismatched totals and tolerate lossy device
  display names without weakening ASCII structure parsing;
- a fake runner covers unsupported versions and missing configured endpoints;
- an ignored test probes a separately supplied, hash-verified v0.3.4 CLI and is
  never required by hosted CI.

The probe tests above do not start the audio server or send PCM. Process
supervision, receiver-loss/Route behavior and physical playback are separate
acceptance steps.

The shared audio-contract tests validate microphone, balanced-speaker and
lossless-music presets; reject voice processing on media playback, invalid QoS,
empty/duplicate/unbounded candidate inventories and unsupported use cases; and
prove deterministic exact candidate intersection without implicit conversion.

The ADR 0041 media-channel tests bind typed Session, Route, Stream and positive
epoch identity to one exact selected stream specification. They prove lossless
PCM frame/packet conversion, bounded opaque encoded packets, wrong-Stream and
wrong-epoch rejection, independent packet-count and aggregate-byte queue
limits, and explicit invalid-payload counters. An integration test drives
opposite microphone and Speaker Routes through separate queues under one
Session and proves pressure on one Route does not mutate the other. These tests
open no socket, run no codec, access no platform API and make no latency or
security claim.

ADR 0042 backend-contract tests reject malformed IDs, empty encoding support,
partial StandardPort declarations, overstated payload visibility and
inconsistent security claims. Audio Share tests drive a bound common PCM packet
through the unchanged TCP/UDP loopback fixture, prove payload preservation and
private-wire segmentation, reject wrong Stream/spec before enqueue, and assert
absent metadata/no security. MicYou tests accept only the conservative voice
lifecycle binding and assert opaque payload/private PCM+Opus/no security; they
do not claim access to or decode a MicYou packet.

ADR 0044 native-LAN tests use one shared Rust/Java golden datagram to lock the
104-byte version-1 header and preserve complete unsigned 64-bit counter
patterns. Rust tests cover single/multi-fragment PCM and encoded payloads,
reverse-order reassembly, duplicates, conflicting metadata, a bounded partial-
packet eviction, wrong binding, malformed/reserved fields, explicit-peer UDP
loopback, spoofed peer rejection and receive timeout. Dependency-free Java
contract tests cover the same codec constants/golden packet, a 1,920-byte
stereo packet split into two datagrams, malformed fields, explicit peer and
UDP loopback. These are local transport-conformance tests: they do not call
Android audio APIs, install an APK or prove remote sound, latency, codec,
loss-recovery or security behavior.

ADR 0045 extends the dependency-free Java contract with packet-count and byte-
capacity queue pressure, wrong-binding rejection, exact PCM packetization,
partial-read handling, sample/time advancement and discontinuity after drops.
The worker composition test moves two 48 kHz stereo packets through bounded
send queue, one-shot UDP sender, two-fragment packets, receiver/reassembly and
bounded sink queue, then checks exact payload/timeline and a two-second-bounded
terminal stop. This still does not invoke `AudioRecord` or `AudioTrack`; Android
application compilation/Lint ensures the worker layer can ship in the APK while
the physical adapters remain disconnected.

ADR 0046 raises the dependency-free native-LAN contract to 107 assertions.
Additional tests cover partial PCM Sink writes, sequence/sample discontinuity
reset, invalid PCM geometry, terminal worker restart and closed literal speaker
configuration. Rust tests cover exact/non-silent tone packets plus bounded
cross-block render accumulation and pressure discontinuity. Android compile and
Lint cover the real `SpeakerSinkAdapter` composition. None of these automated
checks is audible or substitutes for the pending exact-device acceptance.

The MicYou Adapter tests verify exact v2.0.1 CLI identity plus the required
`device-stable-id-v1` capability, bounded structural output-device parsing that
preserves duplicate names but rejects duplicate endpoint IDs, stable-ID/name
configuration, fresh ID-to-index resolution after reorder, exact
ID/index/name child arguments, explicit Wi-Fi bind/port/device arguments,
VoiceInteractive semantic mapping, fixture listener readiness, child status,
bounded diagnostics and idempotent stop/reap behavior. The ignored real-CLI
test requires `CAPYIO_MICYOU_CLI` to name a user-supplied executable built from
the pinned revision. The reviewed Windows CLI validates the same
ID/index/name tuple before and after audio startup. The ignored probe does not
install VB-CABLE or an APK and by itself does not prove a Windows capture
endpoint.

Windows MicYou supervisor tests use a repository fixture to distinguish the
short-lived listener-readiness connection from a retained process-owned TCP
peer, then prove peer close and stopped supervision. Audio Share runs the same
regression through the shared `capyio-process-presence` safe boundary. The
bounded Windows owner table returns counts only; tests and DTOs do not retain
peer addresses.

Microphone desktop-composition tests bind a fake MicYou process boundary to a
real `NodeRuntime` `AdapterManaged` Route. They require three consecutive phone
presence samples before `Active`, keep listener-only readiness at `Starting`,
map peer loss and endpoint/start failures to typed sanitized Problems, stop and
reap the receiver after active peer loss or a bounded initial wait, advance
epoch on retry and prove an active IMU Route remains unchanged. Quick Action
tests require schema v2, a truthful blocked Browser Mock/Tauri state and a
connection hint containing only the validated bind IP/port—not the executable
path or raw endpoint ID.

`physical_quick_action_tracks_disconnect_retry_and_stop` is ignored by default
and requires explicit physical-lab authorization plus `CAPYIO_ADB` and
`CAPYIO_ANDROID_ADB_SERIAL`. Optional bounded `CAPYIO_MIC_*_HOLD_MS` variables
create observation windows. The test force-stops and quick-starts only the
already installed MicYou Android package, drives `Start`, stable `Active`,
phone loss, `Offline`, explicit `Retry`, renewed `Active` and terminal `Stop`,
and leaves the phone package stopped. PCM and exact-silence claims additionally
require an ordinary Windows capture client; TCP presence alone is insufficient.
Any WAV or other recording created for physical acceptance contains raw
microphone payload and must stay in a Git-ignored local evidence directory. It
must never be committed, placed in ordinary logs or uploaded by hosted CI.

MicYou trusted-host configuration tests require a fixed user-local path,
schema-v1 and deny-unknown-fields parsing, complete-or-rejected environment
overrides, exact stable-ID selection even when endpoint names are duplicated,
redacted debug output and create-new persistence that refuses silent overwrite.
Desktop tests prove a file-equivalent configuration installs the same typed
Route while the serialized Quick Action still excludes executable and endpoint
identity. These tests use synthetic paths/inventories and do not run MicYou or
write the real user configuration.

The per-user microphone-host tests use a fake MicYou process boundary to prove
stable peer activation, bounded initial wait, active-peer fail-closed cleanup,
typed endpoint failure, retry generation and invalid address/poll rejection.
Closed-schema tests reject unknown fields and versions and assert that bounded
responses contain neither executable nor endpoint authority. A Windows-only
test creates the owner-scoped remote-rejecting named pipe and performs real
status/start/stop round trips against the fake Runtime. These tests do not run
MicYou, send PCM, modify services/autostart or access a phone. Desktop tests
continue to cover direct trusted-config fallback; physical persistence across
window closure remains a separately authorized deployment acceptance step.

Audio Share regression tests require the `MediaBalanced` common PCM
specification to produce the unchanged pinned protobuf bytes and map only
observable transport counters into common metrics.

A separately ignored real-CLI stale-endpoint test re-probes an explicitly
supplied endpoint that is expected to be absent, requires the typed
`ConfiguredEndpointMissing` result before child spawn and confirms supervisor
state remains stopped. It complements, but never replaces, the ignored current
endpoint start/listen/stop probe.

The next supervisor tests use a repository-built fixture executable to prove
TCP-listener readiness, running/early-exit state, startup timeout, bounded
continuous output, explicit kill/reap, idempotent stop and Drop cleanup. A
separately ignored test briefly starts the hash-verified real Windows CLI on an
explicit loopback port, verifies it remains running after readiness, then stops
and confirms no process/listener remains. Listener readiness is not receiver
presence; v0.3.4 has no machine-readable peer-status API and tests never parse
ordinary log prose into lifecycle state.

The same bounded supervisor has a dedicated virtual-speaker launch contract:
one positional explicit IPv4 bind address, no upstream version/endpoint probe,
listener readiness, bounded output and idempotent stop/reap. Tests reject an
unspecified address and zero port. Desktop projection tests keep this fixed
mode separate from the legacy endpoint picker.

An ignored Windows/Android lab test composes that supervisor with the Runtime
Route. It requires explicit `CAPYIO_VIRTUAL_SPEAKER_EXE`, bind-IP and port host
configuration plus an elevated token, waits for stable receiver presence,
retains a bounded directed-playback window and proves explicit stop. The test
does not install a driver, change permissions or infer audibility from TCP.

Windows-only owner-table tests then prove that the short-lived readiness
connection is not retained as a receiver, a process-owned established peer is
observed, peer close becomes disconnected and stopped supervision becomes
unknown/not-running. The test filters by PID and port and never asserts or
retains peer addresses. This is transport-presence evidence, not Audio Share
negotiation or playback evidence.

Hardware-free desktop composition tests bind a fake process boundary to a real
`NodeRuntime` `AdapterManaged` Route. They require three consecutive established
receiver samples before `Active`, reset the counter on an intervening absence,
map receiver loss, child exit and process-start failure to typed Route Problems,
bound the initial receiver wait, reap the child on wait exhaustion, verify retry
advances the epoch, and prove that audio failure leaves an already active IMU
Route unchanged. They also assert that the AdapterManaged Route exposes a
private-negotiated Audio Share format rather than claiming an unobserved PCM
request. The fake proves orchestration only; the adapter fixture tests remain
the evidence for real child/TCP observation behavior.

The desktop composition tests also map the Adapter's concrete
`ConfiguredEndpointMissing` start error to the stable
`CAPY.AUDIO_SHARE.ENDPOINT_UNAVAILABLE` Problem without retaining the endpoint
ID. Other start failures remain `PROCESS_START_FAILED`; no ordinary CLI log
text is parsed to distinguish them.

Quick Action tests assert schema version 1, a truthful blocked state when host
configuration is absent, finite operations derived from Route state, rejection
of unknown request fields (including an attempted executable path), and matching
Browser Mock/Tauri TypeScript contracts. The Tauri host owns a 250 ms poll loop;
the WebView refresh only observes the projection and is not lifecycle authority.
A separate endpoint-selection contract rejects unknown request fields,
unbounded or non-token input and active-Route process replacement. Display
names are bounded and control characters replaced; inactive replacement is
covered with a fake process boundary. The real ignored CLI probe confirms the
current Windows endpoint inventory remains parseable, while raw IDs are not
asserted or retained in repository evidence.
A separately ignored physical test composes the real supervisor and Runtime
Route and waits for active, disconnect, later-epoch retry, active and stopped.

Windows-service unit tests validate closed launch configuration, explicit
non-unspecified IPv4/port bounds, stable receiver gating, receiver loss,
Broker exit and child-stop ownership without installing a service. The binary
also provides an explicitly time-bounded console mode for a later exact
physical fixture run. SCM install/start/reboot evidence is not implied by these
tests and requires a separately approved service-deployment slice.

The approved `CAPY-AUDIO-001B6A` local-lab run registered the exact manual
LocalSystem service, observed its Broker child plus TCP/UDP ownership, proved a
controlled stop released processes and port 65530, restarted with new process
IDs and regained the Android receiver. A directed five-second CapyIO endpoint
submission then left the service and transport healthy. The run does not claim
human audibility without a separate operator observation and does not cover
reboot/autostart, desktop IPC or installer behavior.

`CAPY-AUDIO-001B6B` adds closed-schema/control-bound unit tests and an ignored
physical desktop/service composition test. The approved local run exercised
dozens of non-administrator status calls across stop/start generations, proved
port cleanup without stopping the SCM host, proved Quick Action service
selection and UI-shutdown independence, restored the Android receiver and
retained `active` state after a five-second endpoint submission. Automatic SCM
configuration was observed; reboot recovery and signed-installer behavior are
still not implied.

## Sidecar smoke test

Adapter Host launches repository-built mock binaries, performs initialize,
probe, health, catalog, Route prepare/start/status/stop/status and shutdown, then
verifies exit and stderr/stdout separation. Separate cases simulate abnormal
child exit, newline-free stdout/stderr overflow and a response that arrives after
the deadline. The terminal-failure cases assert `Poisoned`, child reaping and
future-request rejection. Finite Mock-private samples are not a generic Adapter
contract, data plane or performance test.

## Later platform tests

- Android: actual sensor/audio parameters, permissions, visible service,
  lock/background, focus, route changes and power saving;
- Windows user mode: endpoint enumeration, Broker restart, bounded IPC and
  sleep/resume;
- drivers: install/update/remove, service restart and reboot in an isolated
  VM/dedicated target or the ADR 0029 controlled local lab; project-only
  Verifier remains isolated by default and requires separate approval;
- end to end: IMU Panel/Recorder, audio both directions, camera, gamepad,
  independent Routes, disconnect/reconnect and clock epochs.

## Android audio node shell Gate

`cargo xtask android-check` is the explicit hardware-free 001C Gate. It runs a
dependency-free Java contract executable, Android Lint with warnings denied and
debug APK assembly. The 36 contract assertions cover Port direction, independent
microphone/speaker activation, one-direction Stop, failure isolation, later
generation Retry, stale completion rejection and bounded actual-format/Problem
fields.

ANDROID-A01B1/A01B2 add a second dependency-free contract executable with 16
assertions. It verifies stable module order, immutable catalog snapshots,
Port-owned direction, duplicate rejection, bounded/canonical identifiers and
the rules that an unattached Adapter or absent hardware cannot advertise an
available Capability.
The existing Android host then passes `lintDebug`, `assembleDebug` and
`verifyDebugEntrypoints` with the registry exposed by the service Binder. This
is compile/contract evidence only; the APK is not installed and pending camera,
gamepad, haptics and touchpad entries do not constitute attached Adapters.
The hardware-free host composition executable now has 39 assertions for four
modules, seven non-display Capabilities, attached-versus-pending counts,
Source/Sink direction, hardware presence/absence/unknown, permission state and
live audio, IMU and touch-gamepad lifecycle projection. A separate 40-assertion
lifecycle contract covers generation-bound start, stop, failure, Retry and
stale callback rejection plus complete gamepad-frame bounds, neutralization and
old-generation rejection for non-audio Android Adapters.
The root Android `capyioCheck` task includes this composition executable so the
registry cannot silently disappear from the aggregate Android Gate.

ANDROID-A01C1 adds a hardware-free shared-presentation packaging check. Gradle
builds the exact `apps/desktop` Vue source, stages it as `assets/capyio-ui`, and
requires an HTML entrypoint plus JavaScript and CSS files. The HTML check rejects
root-absolute `/assets/` references so the same Vite output is valid in Tauri and
an Android packaged-asset host. `capyioCheck` depends on this verification and
debug APK assembly; archive inspection additionally confirms all three artifact
classes are present. This is packaging evidence only: the native Activity is
unchanged, no WebView/Tauri observer bridge runs, and no APK is installed.

ANDROID-A01C2 extends the hardware-free composition executable from 39 to 51
assertions. It projects all seven non-display Capabilities into shared schema 4,
checks Android observer mode, node and capability counts, Source/Sink direction,
read-only sharing flags, JSON escaping and a disconnected-service snapshot.
Android Lint and debug compilation cover the local-only WebView host, its single
read-only JavaScript method, the native/shared view switch and Back fallback.
This still provides compile/contract evidence only; no WebView was rendered on
a physical device and no APK was installed.

ANDROID-A01D1 raises the composition executable to 70 assertions. The added
checks cover two schema-v2 `capability_control` Quick Actions, stopped/starting/
active projection, exact available operations, null Route identity, fixed IMU
and gamepad targets, and state-dependent rejection of unknown IDs, invalid
operations, shell-like text and invalid transitions. `capyioCheck` compiles the
real three-method JavaScript bridge and Service Binder calls, then runs Lint and
assembles the debug APK. No physical sensor was opened and no APK was installed.

ANDROID-A01D2 raises the composition executable to 98 assertions. It covers
complete-state composition across buttons, d-pad, both sticks and triggers;
reset behavior; exact field bounds; rejection of unsupported buttons, malformed
unused fields and shell-like kinds; and stopped/active shared-UI JSON projection.
The aggregate Gate also compiles the synchronous generation-bound Binder
submission and packages the shared Controller code. This remains compile,
contract and package evidence: no APK was installed and no physical touch input
was exercised.

ANDROID-A01D3 raises the composition executable to 131 assertions. New checks
cover touchpad attachment, generation-bound Start/Stop, complete-frame sequence
advance, Stop neutralization, stale-generation rejection, zero-to-five contact
parsing, canonical integer syntax, coordinate/pressure bounds, duplicate IDs,
shell-like input rejection and shared-UI state projection. `capyioCheck` also
type-checks/packages the common Vue Touchpad surface and runs Android Lint. This
is hardware-free compile/package evidence; no APK was installed and no physical
multi-touch or Windows projection was exercised.

ANDROID-A01D4 raises the composition executable to 150 assertions. It verifies
five independent Android Capability Quick Actions, permission-required audio
projection, microphone/speaker target separation, lifecycle-valid operation
admission, malformed/unknown rejection, and bounded audio state JSON including
the actual negotiated format and processed-frame count. Lint/debug compilation
cover the bridge handoff back to the Activity permission path. Physical
permission dialogs and audio operation remain covered only by the earlier native
UI device evidence until the shared surface is installed and exercised.

ANDROID-A01D5 raises the composition executable to 170 assertions. It verifies
that the four attached hardware-present Android Sources (microphone, IMU,
gamepad and touchpad) are selectable, while speaker, haptics and the unattached
camera are not. It also covers select/unselect idempotence, exact Node and
capability identity, unknown/null/overlong rejection, and schema-4 selected-state
projection. These checks prove local publication-intent admission only; they do
not prove peer discovery, Route authorization or a data-plane connection.

Offline repository validation additionally pins the Gradle/AGP contract and
checks the source manifest for the approved recording/notification/foreground
permissions, a non-exported dual-type service, disabled backup/cleartext and no
Internet permission. It also requires real `AudioRecord`/`AudioTrack` builders,
`START_NOT_STICKY` and foreground notification ownership. This static check is
not an APK or hardware result.

The local debug APK build and merged-manifest inspection do not themselves
prove device behavior. A separately authorized one-device run now proves the
positive permission flow, actual 48 kHz `AudioRecord`/`AudioTrack` parameters,
concurrent and independent Stop behavior, Activity-finish survival, persistent
notification action and complete service/audio cleanup. Permission denial and
revoke, visible indicator inspection, lock/process death, focus/routing,
long-duration background and vendor power policy remain physical tests. No
microphone recording is retained by the 001C shell.

Gate 7B first proves an unchanged pinned SysVAD build and an approved-target
install as a toolchain/enumeration baseline; synthetic SysVAD
WASAPI loopback is not real-PCM evidence. It then requires `CapyIO Speaker`
enumeration, explicit application selection, endpoint-associated render APO
PCM evidence, bounded ring-full/Broker-loss behavior, silence on the ordinary
physical/RDP endpoint, audio-service/reboot survival and clean uninstall. The
APO callback must have evidence of no blocking, allocation, file/network I/O
or ordinary logging. Repository validation prevents driver source from
appearing while the SysVAD record still declares `source_imported: false`.
ADR 0029 permits `DESKTOP-AT8EVE9` for the approved-target install only after
its recovery, exact-package and rollback preflight passes.

ADR 0051 adds a remote-recording compatibility regression. The speaker-only
case must keep the Android microphone `STOPPED` at zero frames, prove non-zero
Android speaker frames with zero malformed/wrong-source/drop counters, and
verify that a host-audio recorder cannot obtain SysVAD's synthetic tone from
`CapyIO Speaker`. The 21.85 build candidate intentionally rejects that WASAPI
loopback activation; silence or unsupported activation is acceptable, but a
tone is a failure. This test does not claim genuine system-loopback support.

The native Speaker service supervisor is tested independently from physical
audio. Configuration tests reject missing, unspecified, duplicate or
mode-incompatible endpoints. Readiness parsing accepts only the bounded
`native_virtual_speaker=true` first record, output readers remain bounded and
the Runtime can become `active` from native transport readiness while keeping
`receiverPresent=false`. Physical Android counters, not UDP process liveness,
prove remote receipt.

The native microphone slice tests the two halves independently before physical
composition. Capture-ring tests prove its fixed ABI, live-owner/stale-reclaim
rule, whole-block pressure behavior and exact S16LE-mono conversion. Native LAN
tests prove the microphone binding, explicit-peer UDP reassembly and a distinct
bounded readiness record. Service tests require an all-or-none microphone child
configuration, reject it outside native mode and keep startup transactional.
Android contract tests require `AudioRecord` to hand off only through the
bounded packetizer/queue/sender worker. Physical media acceptance requires
non-zero receiver/ring counters with MicYou absent; final projection acceptance
additionally requires a normal Windows recording client to produce a non-silent
WAV from the matching deployed service build.

The controlled 001F2 acceptance records exact Release/APK hashes rather than a
friendly endpoint name alone. It requires 384,000 samples for each eight-second
48 kHz mono run, a non-zero sample count plus RMS/peak, zero client errors,
release of both native UDP ports on Stop, new owners after Start and another
non-silent capture in the new generation. Android sender counters independently
confirm generated/sent packets, datagrams, pressure drops and partial bytes;
raw microphone WAVs remain ignored and are never repository evidence.

The 001G1 orchestration Gate additionally distinguishes pair readiness from
liveness. Unit tests require transactional initial startup, no Stop call on a
surviving child after partial health loss, transition from `active` back to
`starting` without a terminal Problem and unchanged failure/reap behavior when
no child remains. These tests do not replace physical simultaneous-media,
child-exit or reconnect evidence.

## Android read-only lab commands

Android commands require `--serial`; target order is never inferred. They use
an allow-list of `adb devices`, `getprop`, `wm size` and `dumpsys
sensorservice`, impose a four-megabyte process-output bound, and retain only
model/build-version plus bounded sensor-list fields. `android-baseline` prints
sanitized JSON; `android-collect` writes it only below ignored
`test-results/android/<run-id>/`. Neither command installs an APK, grants a
permission, starts a service or changes settings.

The separately authorized physical `CAPY-IMU-001B2` run used the fixed upstream
SensorServer v7.2.1 binary after its published SHA-256 matched. Live evidence
requires paired source timestamps, sequential envelopes, equal Panel/Recorder
counts, zero silent sequence repair, a second clean connection after graceful
close, and an explicit client failure when the phone service stops. Physical
addresses, pairing codes and raw device identifiers are not committed.

The authorized `CAPY-IMU-001B3A` desktop run additionally exercises the Tauri
start/read/stop DTO boundary. Acceptance requires a visible typed failure, a
later successful connection with changing numeric vectors and monotonically
growing sample count, and a stopped state that retains the last snapshot. The
desktop Rust physical test remains ignored by default and requires explicit
`CAPYIO_LIVE_IMU_IP` and `CAPYIO_LIVE_IMU_PORT` environment variables. Normal CI
does not require a phone or a reachable private endpoint.

`CAPY-IMU-001B3B` binds that worker to the same `NodeRuntime` that owns the
desktop Node. Loopback tests assert the staged Route lifecycle, retained
disconnect Problem, fresh retry epoch and explicit stop without a phone. The
ignored physical test asserts real paired samples drive the Route to `Active`
and shutdown reaches `Stopped`. The authorized lab run also confirmed that a
stale phone listener produces `Offline` rather than a false success, then
succeeds after the service is restarted. Private addresses are not retained in
repository evidence.

## Data and timing quality

Signal tests measure latency, clipping, gaps, discontinuities, loss/repeat and
RMS. Drift tests record source/sink samples, queue water level and resampling
ratio rather than inferring drift from acoustic latency. Sensor tests preserve
clock domain, sequence, units, coordinate frame, accuracy and calibration.

## Evidence format

```text
test-results/<run-id>/
  manifest.json
  summary.md
  config.json
  metrics.jsonl
  runtime.log
  adapter-stderr.log
  platform/device inventories as applicable
  input/output recordings only when explicitly authorized
```

`manifest.json` records Git commit, versions, OS/device, Route/Profile/backend,
network mode, case and timestamps.

## CI policy

Required before merge: Rust format, check, Clippy warnings denied, tests,
Protobuf build, docs/repository validation, manifest validation, Adapter smoke,
frontend typecheck/build and dependency/license review when dependencies change.
Hardware jobs may be manual but must attach evidence. Claims match actual runs.

The native DS4 application-consumer Gate compiles and self-tests the SDK-only
`RawGameController` probe without an attached device. Its no-device case must
fail closed. A physical pass is recorded only inside a separately authorized,
exact VID:PID USB/IP attachment window and requires a user-visible control
change; normal CI never attaches a controller.

Pull-request workflows targeting `main` explicitly check out
`github.event.pull_request.head.sha`; a synthetic merge commit is not substituted
for the submitted head. Rust format/check/Clippy/tests, documentation, manifests
and Adapter smoke run on Windows, Linux and macOS. The frontend uses the frozen
pnpm lockfile for typecheck/build. Windows additionally runs native Tauri Cargo
check/build.

Linux/macOS native Tauri packaging is an explicit merge-gate skip in the current
foundation: those runners still execute Rust Core/Adapter and web UI gates, but
they do not count as Tauri application build evidence. Adding non-Windows Tauri
packaging requires an explicit workflow and platform prerequisites; absence is
never reported as a pass.
