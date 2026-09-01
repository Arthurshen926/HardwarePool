# CAPY-CAMERA-001C1 MediaCodec AVC boundary report

- Date: 2026-08-30
- Branch: `codex/capyio-camera`
- Base commit: `fc3da3636ca6c969667e71a9b596dcc944380146`
- Scope: build-only AVC encoder and access-unit boundary; no device execution

## Outcome

The Android camera project now compiles a one-shot surface-input H.264/AVC
MediaCodec owner. The owner configures a closed format, exposes its input
Surface, captures bounded codec-specific data, copies codec output into owned
access units and immediately releases every MediaCodec output buffer.

The encoder is not yet attached to the Camera2 capture session. No MediaCodec
instance ran on a physical device, and there is still no transport or Windows
decoder. This gate proves source/API composition and the buffer contract, not a
valid encoded phone frame.

## Closed contract

- Baseline: 1280×720, 30 fps, 4,000,000 bit/s, one-second key-frame interval.
- Positive even dimensions up to 4096; rate up to 60 fps; bitrate from 64 Kbit/s
  to 50 Mbit/s; key-frame interval up to 10 seconds.
- Surface color request: limited-range BT.709 SDR; maximum B-frames: zero.
- One access-unit payload: at most 4 MiB and owned by the queue item.
- Codec `csd-0`/`csd-1`: at most 64 KiB each.
- Queue: fixed capacity 1–8, non-waiting `tryLock`, drop incoming on contention,
  drop oldest on full, and observable aggregate loss count.
- Encoded bytes remain private `AdapterManaged` data. No Annex-B/AVCC, RTP,
  authentication or public StandardPort claim is made yet.

## Evidence

The expanded command passes:

```text
gradle contractTest :app:assembleDebug :app:lintDebug
```

The JVM contract executable verifies invalid configuration rejection, payload
ownership, codec-config bounds and deterministic queue overflow. Android
compile/lint covers the real MediaCodec API calls. Offline repository
validation rejects network/file/log imports in the encoder callback and
requires the queue's non-waiting lock path.

The resulting C1 debug APK is 2,599,614 bytes with SHA-256
`1e5f23d10dd1867dab01c3ec95383747c27de7414a928f51291c25a0de01fbf5`.
`aapt2 dump permissions` still reports exactly `android.permission.CAMERA`.

## Remaining gates

1. Choose an encoder-supported Camera2 output size and compose preview plus the
   MediaCodec input Surface for one exact approved device.
2. Install the APK on that device after separate approval and record actual
   codec name, profile/level, color metadata, parameter sets and key frames.
3. Specify the private access-unit framing and authenticated transport.
4. Decode on Windows into packed 720p30 NV12 and publish through `camera-host`.
5. Switch the registered MF class factory from fixture to shared ingress and
   repeat the ordinary-camera application test.
