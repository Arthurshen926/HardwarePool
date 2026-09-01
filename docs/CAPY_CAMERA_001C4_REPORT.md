# CAPY-CAMERA-001C4 loopback AVC transport report

- Date: 2026-08-30
- Branch: `codex/capyio-camera`
- Base commit: `fc3da3636ca6c969667e71a9b596dcc944380146`
- Scope: foreground Android CAVC export and loopback-only Windows validation
  receiver over an exact-device ADB reverse tunnel

## Outcome

Android MediaCodec configuration and access units now reach a dedicated worker
through a fixed eight-entry non-waiting queue. The worker recognizes Annex-B,
four-byte-length-prefixed and AVC decoder-configuration layouts, emits the C3
config record before the first accepted key frame and marks sequence-loss
recovery only at a key frame. Its destination is fixed to device loopback port
38173; it performs no discovery, arbitrary-host connection or inbound listen.

The Rust Adapter now reads concatenated CAVC records from a byte stream, caps
payload length before allocation, rejects short headers/payloads and applies
the existing transactional stream guard. The lab executable binds only Windows
`127.0.0.1`, accepts one connection and requires at least one key frame before
reporting success.

The Android manifest now declares exactly CAMERA and the separately authorized
INTERNET permission. It still declares no service, storage, microphone or
location permission. Capture remains user-started and Activity-visible.

## Local evidence

- `cargo test --package capyio-vcamdroid-adapter`: six tests passed.
- `cargo clippy --package capyio-vcamdroid-adapter --all-targets -- -D warnings`:
  passed.
- Android `contractTest`: passed, including layout and loss-recovery cases.
- Android `assembleDebug` and strict `lintDebug`: passed outside the Codex
  filesystem sandbox; sandboxed JDK 25.0.2 ZipFS fails while closing generated
  read-only JARs.
- `python scripts/validate_repository.py`: passed with 84 unique Requirement
  IDs traced.
- `aapt2 dump permissions`: exactly CAMERA and INTERNET.
- The locally built, not-installed APK is 2,602,993 bytes with SHA-256
  `cbb52eab25d9eaf65c4bff3f1587224942ba02d80082f27883705e84b2f402a4`.
- An independent loopback TCP client sent the C3 config/key-frame goldens to
  the built receiver. It reported 1280x720, Annex-B config/data, one key frame,
  seven payload bytes and last sequence 7, then exited successfully.

## Device status and remaining gates

After exact authorization, the hash-recorded APK was installed on the sole
online target `100.66.157.119:33909`, identified by ADB as vivo V2419A / PD2419.
Device-side `sha256sum` and `stat` matched the local 2,602,993-byte APK and
SHA-256 exactly. Package state reported version 0.1.0, target SDK 36 and both
CAMERA and INTERNET granted. The mapping list contained only
`tcp:38173 tcp:38173` for this run.

The user-visible Activity was cold-started and its own `START CAMERA` button was
located through a text-only UI hierarchy and pressed. The Windows receiver then
reported:

```text
stream=7aa9572c1f4663fbb7f5010e2978aef1
epoch=88037297947371
size=1280x720 fps=30 bitrate=4000000
access_layout=AnnexB config_layout=AnnexB
access_units=90 key_frames=4 payload_bytes=601130 last_sequence=104
```

The source sequence ending at 104 while 90 units were accepted demonstrates
that not every vendor output became a delivered record. The receiver did not
relax its C3 rules: gaps reached it only through Android's key-frame plus
discontinuity recovery. No preview pixel or encoded payload was retained.

Cleanup force-stopped the app. Camera Service recorded the disconnect and
reported `Active Camera Clients: []`; the 38173 reverse mapping was removed and
the subsequent reverse list was empty. The installed app remains present but
force-stopped. This run proves real vendor AVC bytes reach and pass validation
on Windows. It does not prove Windows AVC decode, packed NV12 publication or
virtual-camera visibility; those are subsequent slices.
