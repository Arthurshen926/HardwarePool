# CAPY-AUDIO-NATIVE-001D2 implementation report

Date: 2026-08-31

Status: hardware-free Android media-worker composition complete; platform audio
and physical remote sound not yet connected

## Outcome

The Android native-LAN boundary now covers more than packet encoding. It can
turn frame-aligned PCM worker reads into exact common packets, move them through
a bounded sending queue and one-shot UDP worker, reassemble them on a separate
receiving worker, and deliver complete immutable packets to a bounded sink
queue. The composition has no microphone/speaker role and can be instantiated
independently for either directed Route.

The test path moves two 48 kHz stereo PCM packets end to end over local UDP and
verifies exact bytes, sequence, source timestamp and first-sample index before
stopping both workers. It is synthetic worker evidence, not phone/Windows or
audible-media evidence.

## Bounds and failure behavior

- queue: 1–128 packets and at most 4 MiB aggregate bytes; the original
  64-packet bound was raised by 001E physical burst evidence;
- PCM push: at most 256 KiB per call and exact frame alignment;
- format: 8–384 kHz, 1–32 channels, 2–4 bytes/sample and integral 2.5–60 ms
  packet duration;
- reassembly: 1–8 partial packets and the ADR 0044 datagram/fragment limits;
- wait/deadline: queue 0–2,000 ms; endpoint 1–2,000 ms;
- worker shutdown: close/interrupt plus at most a 2-second join;
- queue packet/byte pressure and wrong binding are distinct counters/outcomes;
- pressure drops advance sequence/sample/time and make the next accepted packet
  discontinuous;
- endpoint, queue and reassembler binding mismatch fails before thread start;
- receiver queue pressure is observable and a sequence gap remains available to
  the later jitter/playback policy;
- I/O failure reports only stable `CAPY.ANDROID.LAN_SEND_IO` or
  `CAPY.ANDROID.LAN_RECEIVE_IO`, without exception/log text.

## Architecture boundary

All networking remains in `NativeLanSenderWorker`,
`NativeLanReceiverWorker` and `NativeLanUdpEndpoint`. Existing
`MicrophoneSourceAdapter`, `SpeakerSinkAdapter` and `AudioNodeService` contain
no network API and have not been wired to these queues. The application has no
peer/Route configuration UI, binder command, automatic connection or persisted
network authority.

The layer uses Java platform APIs only. No production dependency, codec,
third-party source or binary was added.

## Automated evidence

- `:node-contract:nativeLanContractTest --rerun-tasks`: PASS — 85 assertions;
  - Rust/Java golden wire and unsigned counters;
  - reverse-order reassembly and duplicate handling;
  - packet/byte queue pressure and wrong binding;
  - PCM packetization, timeline advancement and discontinuity recovery;
  - explicit-peer UDP and spoof/broadcast rejection;
  - two-packet sender/receiver worker composition and bounded one-shot stop;
- `cargo xtask android-check`: PASS;
  - 36 audio lifecycle assertions and 85 native-LAN assertions;
  - application and contract Java compilation;
  - Lint with warnings denied;
  - debug APK assembly;
- `cargo xtask ci`: PASS;
  - format, workspace check and warnings-as-errors Clippy;
  - 181 Rust tests passed and 4 external/physical tests remained explicitly
    ignored;
  - IMU demo, documentation/manifests, Adapter smoke, repository structural
    validation (88 traced Requirement IDs), desktop typecheck and production
    build;
- `git diff --check`: PASS;
- no ADB, APK installation, device permission, Windows service or driver action
  occurred.

## Next slice

The next implementation choice should be narrow rather than enabling both
directions at once:

1. connect the speaker receive queue to a dedicated `AudioTrack.write` worker,
   expose a trusted lab Route configuration outside the Activity, and compare
   it against the proven Audio Share path (`001E`);
2. after speaker disconnect/retry and audible evidence pass, connect
   `AudioRecord` reads to the packetizer/sender and compare against MicYou
   (`001F`);
3. retain independent Route authorization/generation, then test concurrent
   duplex and partial failure (`001G`).

APK installation and physical testing remain separate high-risk steps requiring
exact-package/device authorization.
