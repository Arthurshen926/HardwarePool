# CAPY-AUDIO-000/001A0 Report

Date: 2026-08-24

Status: local implementation complete; Runtime Route supervision not started

## Delivered

- pinned Audio Share v0.3.4 at commit
  `342751fe675367483170b002ec6054e243966dc0` under Apache-2.0;
- verified official Windows ZIP and Android APK SHA-256 values in the third-party
  record, without importing either artifact;
- recorded that the official Windows executable lacks Authenticode signing;
- validated explicit IP-literal bind address, non-zero port, bounded playback
  endpoint ID, encoding, channel count and sample rate;
- generated only direct process arguments, never a shell command;
- implemented bounded stdout/stderr readers, five-second default deadline,
  process kill/reap on timeout and sanitized failure diagnostics;
- parsed the pinned version and endpoint inventory with 64 KiB output, 1 KiB
  line, 64 endpoint, 256-byte ID and 512-byte name limits;
- kept endpoint display names lossy when Windows console encoding is unavailable
  while preserving strict ASCII structure and ID validation.

## Physical upstream spike

The separately authorized lab used only the hash-verified unmodified release.
The Windows CLI was explicitly bound rather than accepting its incorrect
default interface selection. The Android receiver negotiated float stereo PCM,
received UDP data and heartbeats, closed after pause, then accepted a second
clean peer/start. Android `AudioFlinger` reported an app-owned matching
`AudioTrack` and increasing written frames. This proves upstream transport and
platform playback submission, not subjective audibility or long-duration
quality.

Private addresses, device identifiers and endpoint display names are not
retained in repository evidence.

## Automated evidence

- `cargo test -p capyio-audio-share-adapter`: 7 passed, 1 ignored;
- ignored real-CLI probe with explicit `CAPYIO_AUDIO_SHARE_EXE`: 1 passed;
- `cargo check -p capyio-audio-share-adapter`: passed;
- `cargo clippy -p capyio-audio-share-adapter --all-targets -- -D warnings`:
  passed;
- `cargo xtask validate-docs`: 84 unique Requirement IDs passed;
- `cargo xtask validate-manifests`: 2 existing distributable manifests passed.
- `cargo xtask ci`: passed, including 104 Rust tests, repository validation,
  Adapter smoke and frontend typecheck/build;
- Windows `cargo check -p capyio-desktop` and `cargo build -p capyio-desktop`:
  passed (the MSVC linker emitted its normal import-library informational
  warning).

## Remaining risks and next slice

- the upstream executable is unsigned and is not distributed by CapyIO;
- the current probe does not start or supervise the server;
- receiver loss is not yet mapped into a Runtime Problem/`Offline` Route;
- retry epoch, independent IMU state and generic Quick Action projection remain
  unimplemented;
- Android background/audio-focus behavior and audible quality require a person
  beside the receiver and longer retained tests.

Next: `CAPY-AUDIO-001A1`, a bounded child supervisor with explicit stop/reap and
typed failure mapping, still keeping upstream TCP/UDP PCM outside CapyIO control.
