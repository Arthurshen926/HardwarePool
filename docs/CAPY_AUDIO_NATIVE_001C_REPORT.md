# CAPY-AUDIO-NATIVE-001C implementation report

Date: 2026-08-31

Status: local implementation/build Gate complete; authorized one-device
physical acceptance complete

## Outcome

The repository now contains one CapyIO-owned Android application shell with an
independent microphone Source and speaker Sink. A non-exported foreground
service, rather than the Activity, owns both platform resources. The app can be
built into a debug APK without Audio Share, MicYou, Compose, AndroidX, codec or
network runtime dependencies.

The exact debug APK was also installed on one authorized vivo V2419A running
Android 16/API 36. Runtime permission, real `AudioRecord`/`AudioTrack`,
concurrent capability, independent Stop, Activity-finish and notification Stop
behavior passed. This is platform-endpoint evidence only: no media crossed the
network and no remote sound was produced.

This slice does not replace either compatibility application. It has no native
transport, Rust/JNI binding, pairing or `INTERNET` permission.

## Implemented boundaries

- schema-v1, payload-free Node snapshot with a private persistent Node UUID;
- `capyio.audio.frames/1` microphone Source and speaker Sink Port identities;
- independent `Stopped/Starting/Active/Stopping/Failed` state and generation;
- stale completion rejection, isolated failure and later-generation retry;
- user-requested `RECORD_AUDIO` and `POST_NOTIFICATIONS` handling;
- microphone/media-playback foreground-service declaration and live type mask;
- persistent notification with active capability text and `Stop all` action;
- real `AudioRecord` initialization and bounded preallocated blocking read
  worker; bytes are counted then discarded;
- real empty `AudioTrack` initialization and actual-format/underrun observation;
- sanitized fixed Problem codes without exception text or audio logs;
- disabled app backup and cleartext networking, non-exported service and
  `START_NOT_STICKY` restart policy.

## Build dependency record

- Gradle 9.5.0 `all` distribution, pinned wrapper SHA-256
  `a3c4ba4aca8f0075688b9c5b18939fd28e8cb4357c227da5c1d9f38343791439`;
- Android Gradle Plugin 9.3.1;
- Android compile/target SDK 36 and Build Tools 36.x;
- Java 17 source/target bytecode;
- no third-party APK runtime dependency.

The pins are recorded by ADR 0043. Version-reminder Lint checks are disabled for
these explicit pins; all functional, API, manifest, resource, accessibility and
security Lint findings remain warnings-as-errors.

## Automated evidence

- `cargo xtask android-check`: PASS;
  - 36 dependency-free lifecycle/validation assertions;
  - Android Java/resources/manifest compilation;
  - Android Lint with warnings denied;
  - debug APK assembly;
- merged APK manifest inspection: PASS;
  - application `dev.capyio.android`, min 26, target 36;
  - exactly the five intended permission names;
  - `AudioNodeService` non-exported, `stopWithTask=false`, foreground type mask
    `microphone|mediaPlayback`;
  - cleartext and backup disabled;
- `cargo test -p xtask`: PASS — 2 tests;
- `python scripts/validate_repository.py`: PASS — 88 Requirement IDs, including
  the new offline Android shell/pin contract.
- `cargo xtask ci`: PASS — format, workspace check, Clippy with warnings
  denied, 173 Rust tests with 4 explicit physical/external ignores, IMU fixture,
  docs/manifests, Adapter smoke, repository validation and desktop
  typecheck/build. Android compilation remains the separate passing
  `android-check` Gate above.

The generated debug APK remains ignored at
`platform/android/app/build/outputs/apk/debug/app-debug.apk`. It was not copied
into source control or described as a release artifact.

## Authorized physical-device evidence

The project owner explicitly authorized installing/overwriting the 001C debug
APK and exercising recording, notification and foreground-service behavior on
the paired personal phone. The unique online ADB target and APK were verified
before installation. No device address, persistent Node UUID or captured audio
is retained in repository evidence.

- APK SHA-256:
  `0F5F8D6530D7FC9102ED6A216D8A8D2D7C39B3FAF61CB482D5611CBCBC227D63`;
- debug signer certificate SHA-256:
  `29307555E525E46A29150BB85D0B73E3FDE7270865B215D07B5A95805CC41007`;
- fresh install of `dev.capyio.android` version `0.1.0-dev`: PASS;
- system permission UI and resulting `RECORD_AUDIO` plus
  `POST_NOTIFICATIONS` grants: PASS;
- microphone Source: PASS — Android reported 48 kHz, mono, PCM16 and a
  1,920-frame burst; the counter advanced beyond eight million frames while
  every captured byte was discarded;
- speaker Sink: PASS — Android reported 48 kHz, stereo, PCM16 and a
  2,052-frame burst; AudioService recorded `AudioTrack` start and release;
- concurrent activation: PASS — the foreground-service type mask became
  microphone plus media playback (`0x82`);
- independent Stop: PASS — stopping the speaker retained active microphone
  capture and a microphone-only type mask; stopping the microphone retained
  the active speaker Track and a media-playback-only type mask;
- Activity ownership boundary: PASS — the speaker foreground service remained
  active after the Activity was finished with Back;
- persistent notification and `全部停止` action: PASS — the action remained
  available without the Activity and removed the service, notification and
  AudioTrack;
- cleanup: PASS — no active CapyIO service or running recording AppOp remained,
  and package-scoped logs contained no crash, ANR or selected runtime/security
  exception.

The debug package remains installed for subsequent development, but both audio
capabilities and its foreground service were stopped at the end of the test.

## Unresolved risks and next slice

- One phone/OS combination is not a compatibility matrix. Permission denial
  and revoke, visible microphone-indicator inspection, lock screen, process
  death, vendor power policy, audio focus/routing changes and long-duration
  background behavior remain unqualified.
- The microphone worker deliberately discards audio and the speaker Track has
  no packet writer, so neither direction can carry remote sound.
- Java DTOs are not yet bound to the Rust `AudioMediaStreamBinding`; Session,
  Route, Stream and epoch arrive with the native backend/JNI boundary.
- Debug signing is development-only. Distribution signing/install/update work
  is outside 001C.

`CAPY-AUDIO-NATIVE-001D` should select one replaceable native backend, record
its dependency/security contract and connect it to bounded queues before any
physical switchover in 001E/001F.
