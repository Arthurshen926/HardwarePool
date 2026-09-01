# CAPY-AUDIO-NATIVE-001 — CapyIO native audio subsystem

Status: active

Owner: Codex and project owner

Created: 2026-08-31

Depends on: completed Speaker and microphone functional Gates, ADR 0035

Current status: `CAPY-AUDIO-NATIVE-001A/001B/001C/001D1/001D2` are complete locally on
2026-08-31. Evidence is in `docs/CAPY_AUDIO_NATIVE_001A_REPORT.md`,
`docs/CAPY_AUDIO_NATIVE_001B_REPORT.md` and
`docs/CAPY_AUDIO_NATIVE_001C_REPORT.md`, plus
`docs/CAPY_AUDIO_NATIVE_001D1_REPORT.md` and
`docs/CAPY_AUDIO_NATIVE_001D2_REPORT.md`. The authorized 001C one-device
endpoint/lifecycle qualification also passed. `001D1/001D2` supply a bounded
local UDP reference, Rust/Java golden wire and Android packetizer/queue/worker
composition. The speaker-first `001E` slice is functionally accepted on the
controlled Windows/Android pair.

`001F1` now connects Android `AudioRecord` to the common packet and native LAN
sender, and connects the Windows native receiver to the extracted capture-ring
crate under ADR 0047. A physical run committed 228,960 real microphone frames
from 477 packets with zero observed transport/ring drops and no MicYou process.
The matching dual-child service build was then deployed and `001F2` accepted:
three ordinary Windows-client WAV captures were non-silent, both UDP ports were
released/reacquired across generation 1/2, and simultaneous Android microphone
and speaker ownership reported zero microphone queue drops.

`001E` now has an accepted Android `AudioTrack` receiver, native tone sender
and service-owned Windows render-ring Broker under ADR 0046. Native tone,
ordinary Windows playback, phone capability restart and Windows service
stop/restart all passed with non-zero exact counters and no observed drops.
Release soak, sender-loss and installer qualification remain separate work.
ADR 0049 and the build-verified 21.83 package retire the stale MicYou render
ingress. The signed package is installed; interactive-session regression opens
the capture endpoint and proves non-zero Android microphone media. ADR 0050
rejects the later 21.84 float-pin experiment and records the no-reboot rollback
to 21.83.

A speaker-only UU Remote reproduction now excludes full-duplex feedback: the
microphone stayed stopped at zero frames while the speaker Sink received
384,000 frames without packet errors. ADR 0051 identifies the inherited SysVAD
synthetic loopback tone and defines a 21.85 fail-closed build candidate. Its
controlled-host deployment and speaker/microphone/UU regression are pending;
real Windows system-loopback capture is outside this closing slice.

The signed 21.85 package is now installed without reboot. Direct loopback
activation fails closed, while separate and simultaneous physical speaker and
microphone regressions passed with non-zero media and no reported Android
transport errors/drops. Test cleanup stopped both Routes and retained the
healthy Windows services. The owner then confirmed normal Android playback and
no high-frequency squeal through UU Remote. This closes the 001G controlled-host
compatibility slice; genuine loopback capture, security hardening and longer
qualification remain later work.

The controlled 21.85 run also found that UU reserves a non-enumerated Windows
UDP interval covering the former 46000-series lab ports. No socket-sharing
workaround was retained. The lab configuration moved to speaker 40000/40001
and microphone 40010/40011, after which a complete Stop/Start acquired new
child PIDs and both ports while UU remained running. Android and Windows
bidirectional media counters passed with no transport drops or errors. Human UU
listening then confirmed normal Android playback without the previous squeal,
so this compatibility slice is accepted.

`001F1/001F2` now have a physically accepted native microphone data plane and
matching installed service lifecycle. The native route remains a controlled-
lab configuration rather than a production-secure/default distribution.

## Objective

Replace the two default external Android/Windows compatibility applications
with one CapyIO-owned, direction-neutral audio subsystem while preserving the
already proven Speaker and microphone paths as regression baselines.

The target product has one CapyIO Android Node application. Its microphone
Source and speaker Sink remain independent Capabilities and Routes, but both
use the same selected-stream, packet, buffering, timing, metrics and transport
backend contracts.

## Migration slices

1. `CAPY-AUDIO-NATIVE-001A` — define the transport-backend-neutral media seam:
   bind Session, Route, Stream, epoch and exact `AudioStreamSpec`; carry bounded
   PCM or encoded packets; provide a bounded worker-thread reference queue and
   hardware-free conformance tests.
2. `CAPY-AUDIO-NATIVE-001B` — adapt the Audio Share and MicYou compatibility
   paths to that seam without changing their pinned private wire contracts.
   They remain `AdapterManaged` golden baselines.
3. `CAPY-AUDIO-NATIVE-001C` — create one CapyIO Android Node/service shell with
   independently controlled microphone Source and speaker Sink platform
   adapters. Android permission and foreground-service manifest changes require
   their separately approved high-risk step.
4. `CAPY-AUDIO-NATIVE-001D` — implement and measure at least one replaceable
   native network backend. An AOO integration may be spiked behind an Adapter,
   pinned and provenance-recorded; it is not assumed to be the final backend.
   `001D1` fixes the wire/reference endpoint and `001D2` fixes bounded Android
   packetization, queues, reassembly and one-shot media workers.
5. `CAPY-AUDIO-NATIVE-001E/F` — switch the Speaker Sink and microphone Source
   respectively to the CapyIO Android application, retaining one rollback path
   and physical regression evidence for each direction.
6. `CAPY-AUDIO-NATIVE-001G` — exercise both independent Routes concurrently,
   including partial failure, focus/permission changes and optional render
   reference association without merging authorization or lifecycle.
   `001G1` now separates Windows native-pair liveness from readiness so a
   runtime exit in one child does not stop the surviving Route. Bounded
   per-direction recovery, diagnostics and physical interruption evidence
   remain later 001G slices.
   Symmetric controlled child-exit evidence now proves the surviving Windows
   direction keeps its PID and UDP owner. The apparent phone-inbound failure was
   a foreground UI refresh defect: 0.4.3-dev received 1,000 datagrams, completed
   500 packets and rendered 240,000 frames without an Activity rebind. Full
   concurrent media remains open only as a single bundled two-direction
   acceptance run. Installed 21.83 exposes one speaker and one microphone; the
   interactive user-session probe opens the microphone and receives non-zero
   Android media. Sandbox-only `E_INVALIDARG` results are invalid unless a
   physical capture device succeeds in the same account.
7. `CAPY-AUDIO-NATIVE-001H` — make the CapyIO-native paths the default and move
   Audio Share/MicYou to explicit compatibility-only packaging after parity,
   security and migration evidence pass.

## Completed `001C` acceptance

1. One buildable `dev.capyio.android` application owns a stable private Node ID
   and declares microphone Source and speaker Sink Ports under the common audio
   Profile.
2. A non-exported foreground service owns platform resources independently of
   Activity lifetime and exposes one persistent Stop action.
3. Microphone and speaker have independent generation-bound lifecycle, actual
   format, metrics and sanitized failure state.
4. Microphone start requires granted recording and notification permission;
   the complete foreground-service declarations are present.
5. `AudioRecord` uses one bounded preallocated read worker and discards payload;
   `AudioTrack` is initialized without pretending a transport exists.
6. At 001C acceptance the manifest had no network permission or
   cleartext/backup surface; 001D1 later added only the approved `INTERNET`
   permission while retaining the other restrictions.
7. Dependency-free contract tests, Lint and debug APK assembly pass through
   `cargo xtask android-check` without installing or controlling a device.
8. On one authorized Android 16 device, runtime permission, actual 48 kHz
   `AudioRecord`/`AudioTrack`, concurrent activation, independent Stop,
   Activity-finish survival and notification `Stop all` cleanup pass.

## Completed `001D1` acceptance

1. One direction-neutral `AdapterManaged` backend consumes the ADR 0041 common
   packet for either Route direction and preserves all identity/timeline fields.
2. Version-1 UDP datagrams have a fixed 104-byte header, a 1,200-byte datagram
   ceiling, canonical fragmentation and a 70,144-byte packet ceiling.
3. Reassembly is limited to 1–8 partial packets and exposes duplicates,
   malformed data, wrong bindings and capacity eviction.
4. Endpoints accept one explicit unicast IP/port peer and a 1–2,000 ms deadline;
   unexpected peers and oversized/malformed datagrams fail closed.
5. Rust and dependency-free Java codecs match one committed golden datagram and
   preserve the complete unsigned 64-bit counter pattern.
6. Android networking resides in a media-worker boundary; `AudioRecord` and
   `AudioTrack` callback/worker classes contain no socket calls.
7. The approved Android `INTERNET` permission is present, with cleartext app
   traffic still disabled and no new APK runtime dependency.
8. The backend declares authentication, confidentiality, integrity, replay and
   downgrade protection absent; it is restricted to a trusted lab.

## Completed `001D2` acceptance

1. An exact PCM packetizer accepts bounded frame-aligned worker reads and
   preserves sequence, timestamp, sample index and discontinuity.
2. Direction-neutral packet queues have independent packet-count and aggregate-
   byte bounds, non-blocking offer and explicit pressure/wrong-binding outcomes.
3. Queue drops advance the timeline and mark the next accepted packet as
   discontinuous; explicitly discarded partial audio also advances time.
4. Android reassembly mirrors the 1–8 Rust partial-packet bound and rejects
   conflicting metadata/data.
5. Sender and receiver own sockets only on one-shot background workers, report
   stable failures and stop via close/interrupt plus a two-second join bound.
6. Endpoint/queue/reassembler Route bindings must match before a worker starts.
7. A dependency-free local UDP test moves two stereo PCM packets from
   packetizer through both bounded queues and verifies exact payload/timeline.
8. Android compile, 85 LAN assertions, Lint and debug APK assembly pass without
   ADB, installation or device mutation.

## Retained non-scope after `001F2`

- no independent dual-Route service control, per-direction automatic recovery
  or Desktop microphone metrics;
- no independent per-direction Desktop service controls or retained diagnostics
  across a stopped Android generation;
- no codec implementation, jitter deadline, retransmission, congestion
  control, resampling, clock correction, discovery or reconnect;
- no pairing, authorization binding, authenticated encryption or production
  StandardPort claim;
- no AOO/SonoBus source or binary import.

## Retained non-scope after `001C`

- no JNI/Rust Runtime binding, socket, codec, pairing or production security;
- no remote sound in either direction and no compatibility-app removal;
- no permission-denial/revoke, indicator, lock/process-death, vendor-power,
  focus/routing, latency, quality or concurrent-network-Route evidence;
- no release signing, distribution or automatic update; the authorized debug
  install is development-only.

## Completed `001A` acceptance

1. A media-stream binding contains typed Session, Route and Stream IDs, a
   positive epoch and one validated exact `AudioStreamSpec`.
2. The direction-neutral packet contract represents PCM and encoded payloads
   without importing a codec or network implementation.
3. Packet and queue payload/count/byte bounds fail explicitly; wrong Stream or
   epoch data never enters the queue.
4. PCM conversion preserves sequence, timestamps, sample index, discontinuity
   and bytes exactly.
5. Opposite microphone and Speaker Routes may share one Session while retaining
   distinct bindings, formats, queue capacity and failure state.
6. Tests require no phone, APK, driver, socket, codec or platform SDK.

## Retained non-scope for `001A`

- no public network byte layout, socket, AOO, RTP, QUIC or WebRTC dependency;
- no production pairing, encryption, replay window or downgrade binding claim;
- no Android project, permission, foreground service or APK operation;
- no Windows driver/APO/ring, Audio Share wire or MicYou wire change;
- no 96 kHz, multichannel, DSP, AEC or acoustic-quality claim;
- no removal of either physically proven compatibility path; native and MicYou
  capture producers remain mutually exclusive.

## Completion bar for the overall plan

- one CapyIO Android application exposes independently controllable microphone
  Source and speaker Sink Capabilities;
- both directions use one backend-neutral media contract and at least one
  CapyIO-owned backend without Audio Share or MicYou in the default runtime;
- ordinary Windows applications still see the existing CapyIO virtual devices;
- physical tests cover each direction, concurrent use, disconnect/retry,
  permission/focus/background behavior, bounded queues and common metrics;
- transport security and distribution claims match retained evidence;
- compatibility implementations retain upstream, revision, license, imported
  paths and local-modification records until they are removed.
