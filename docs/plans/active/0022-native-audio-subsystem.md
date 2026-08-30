# CAPY-AUDIO-NATIVE-001 — CapyIO native audio subsystem

Status: active

Owner: Codex and project owner

Created: 2026-08-31

Depends on: completed Speaker and microphone functional Gates, ADR 0035

Current status: `CAPY-AUDIO-NATIVE-001A/001B/001C` are complete locally on
2026-08-31. Evidence is in `docs/CAPY_AUDIO_NATIVE_001A_REPORT.md`,
`docs/CAPY_AUDIO_NATIVE_001B_REPORT.md` and
`docs/CAPY_AUDIO_NATIVE_001C_REPORT.md`. `001D` is next. The authorized 001C
one-device endpoint/lifecycle qualification also passed; transport and remote
sound remain absent.

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
5. `CAPY-AUDIO-NATIVE-001E/F` — switch the Speaker Sink and microphone Source
   respectively to the CapyIO Android application, retaining one rollback path
   and physical regression evidence for each direction.
6. `CAPY-AUDIO-NATIVE-001G` — exercise both independent Routes concurrently,
   including partial failure, focus/permission changes and optional render
   reference association without merging authorization or lifecycle.
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
6. The manifest has no network permission and no cleartext/backup surface.
7. Dependency-free contract tests, Lint and debug APK assembly pass through
   `cargo xtask android-check` without installing or controlling a device.
8. On one authorized Android 16 device, runtime permission, actual 48 kHz
   `AudioRecord`/`AudioTrack`, concurrent activation, independent Stop,
   Activity-finish survival and notification `Stop all` cleanup pass.

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
- no removal of either physically proven compatibility path.

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
