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
