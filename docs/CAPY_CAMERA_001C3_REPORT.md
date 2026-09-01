# CAPY-CAMERA-001C3 private AVC record report

- Date: 2026-08-30
- Branch: `codex/capyio-camera`
- Base commit: `fc3da3636ca6c969667e71a9b596dcc944380146`
- Scope: cross-language encoded-camera record contract; no network or APK update

## Outcome

The reserved VCamdroid Adapter now owns a versioned private AVC config/access-
unit record and a strict Rust decoder. The Android-free camera contract owns the
matching Java encoder. Both implementations produce the same config and key-
frame golden bytes.

The 56-byte big-endian header binds every record to one non-zero 16-byte stream
ID and positive epoch, then carries a kind, closed flags, sequence, presentation
time and exact payload length. Configuration declares bounded dimensions, rate,
bitrate, access-unit/CSD layout, the fixed limited-range BT.709 SDR preset and
two bounded codec-data blobs. Access units remain capped at 4 MiB.

The Rust guard rejects data before config, wrong stream/epoch, duplicate/replay
sequence, timestamp regression, unmarked gaps, discontinuities without a key
frame, duplicate config and data after end of stream. Rejected input does not
advance guard state.

## Safety boundary

- This is `AdapterManaged`, not `capyio.video.frames/1`.
- No socket, listener, peer address, Android permission, service or APK update
  is added.
- No encoded bytes enter Core, Protobuf, JSON-RPC, logs, files or the Windows
  kernel boundary.
- The record is not encryption or authentication. Production exposure remains
  blocked on an authenticated Route/Session-bound transport.
- Vendor Annex-B/length-prefixed layout detection is explicit future work; the
  decoder never guesses from payload bytes.

## Evidence

The targeted Rust tests and warnings-as-errors lint pass:

```text
cargo test --package capyio-vcamdroid-adapter
cargo clippy --package capyio-vcamdroid-adapter --all-targets -- -D warnings
```

The Android-free executable contract test compiles main/test sources together
to avoid a JDK 25.0.2 Windows ZipFS close defect, then passes the identical
golden vectors:

```text
gradle --offline contractTest :app:compileDebugJavaWithJavac
```

No production or test dependency was added.

The C3 debug APK is 2,609,014 bytes with SHA-256
`a36f522b0de562ff16585083bf5f5724ef9804963b839dacb27b88cc82dc3136`.
`aapt2 dump permissions` still reports exactly `android.permission.CAMERA`.
This APK was not installed; the approved device remains on the separately
hash-recorded C2 build.

## Remaining gates

1. Detect and record actual MediaCodec CSD/access-unit layout, config, first key
   frame and drop counters on the explicitly selected device.
2. Select and review an authenticated transport; adding Android INTERNET or a
   service remains a separately authorized permission/lifecycle change.
3. Feed decoded AVC into packed 720p30 NV12 and the existing Windows
   `camera-host` mapping.
4. Switch registered Media Foundation activation from the fixture to the shared
   consumer and repeat an ordinary-camera-application roundtrip.
