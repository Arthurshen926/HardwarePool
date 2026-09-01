# CAPY-CAMERA-001C9 — Camera latency hardening report

Date: 2026-08-30

Status: implementation and bounded physical-device validation complete; exact
glass-to-glass latency remains unmeasured.

## Trigger

The user reported that the Android-to-Windows camera preview felt delayed after
the CAPY-CAMERA-001C8 Windows inbox Camera validation. This slice reduces known
queueing sources and makes both codec low-latency state and decoder backlog
observable. It does not claim a measured end-to-end latency improvement.

## Changes

- The Android encoder requests `MediaFormat.KEY_LATENCY = 1`, realtime
  `KEY_PRIORITY = 0`, the selected frame rate through
  `KEY_MAX_FPS_TO_ENCODER`, and zero B-frames. The encoder name and the latency
  value reported in its output format are exposed in the foreground status UI.
- The Android encoder access-unit queue shrinks from four entries to two. The
  network-export queue shrinks from eight entries to two and changes from
  drop-newest to drop-oldest, so overload retains recent rather than stale
  access units.
- The Windows inbox H.264 MFT enables `CODECAPI_AVLowLatencyMode`, reads the
  value back through `ICodecAPI`, and fails closed when the setting is absent or
  not retained. The lab reports both that state and its maximum decoder pending
  sample count.
- `camera-latency-clock.html` provides a local high-contrast millisecond clock
  for a later phone-to-screen measurement without network content or retained
  camera frames.

At 30 fps, the two application queue capacities fall from twelve access units
in total to four. Capacity alone corresponds to a theoretical upper queue
residence bound falling from about 400 ms to about 133 ms; this is not a
measurement and excludes capture, codec, transport, Media Foundation, Frame
Server, display and scheduling delay.

Android documents `KEY_LATENCY` as an optional encoder latency request measured
in frames whose actual value should be checked in the output format, and
`KEY_PRIORITY = 0` as realtime priority. Microsoft documents
`CODECAPI_AVLowLatencyMode` as the low-latency encoder/decoder setting for
realtime communications.

- <https://developer.android.com/reference/android/media/MediaFormat>
- <https://learn.microsoft.com/windows/win32/medfound/codecapi-avlowlatencymode>

## Automated evidence

- The Android no-dependency contract test passed after changing the baseline
  encoded queue capacity to two.
- Gradle 9.5.0 ran `contractTest :app:assembleDebug :app:lintDebug --offline`
  successfully. The official Gradle archive was used only below ignored
  `target/`; SHA-256:
  `553c78f50dafcd54d65b9a444649057857469edf836431389695608536d6b746`.
- The Windows decoder low-latency regression test passed and Clippy accepted all
  `capyio-vcamdroid-adapter` targets with warnings denied.
- The rebuilt debug APK SHA-256 is
  `7403B8A3B6547F45BDB5D2F0EB4CD6B0EA0A9EC33B0294F06C3D97FC0F94A7C5`.
  Its manifest declares exactly CAMERA and INTERNET.

## Physical-device and Windows evidence

The explicitly selected `V2419A` at `100.66.157.119:36275` accepted the updated
APK and retained its existing CAMERA and INTERNET grants. Two bounded streams
were exercised through the authenticated ADB reverse tunnel and the temporary
Session/CurrentUser `CapyIO Camera` registration on `DESKTOP-AT8EVE9`:

- stream `6524234dd159fc4013738498a8de0c77`: 3,600 access units decoded and
  published, last source sequence 3,799, distinct first/last checksums,
  `decoder_low_latency=true`, and `max_decoder_pending_samples=0`;
- stream `c2553d8db843077a0efa3e904df0e38a`: Windows inbox Camera displayed a
  continuous live V2419A scene; 3,562 access units decoded and published through
  source sequence 3,775 with distinct first/last checksums,
  `decoder_low_latency=true`, and `max_decoder_pending_samples=0`.

Both fixed 60-second GUI holds reported pass and cleanup pass. No screenshot or
camera frame was retained. The updated Android process configured and streamed
successfully, but the vendor encoder's output-format latency value could not be
reliably extracted from the active `TextureView`; therefore only the request,
not the vendor-reported value, is evidenced.

The local clock harness was opened, but the phone was not aimed at it during the
final Windows Camera run. There is consequently no valid end-to-end millisecond
sample from this slice.

## Rollback and residual risk

Final checks found no deployed DLL, lab directory, fixed CLSID, virtual-camera
registration, lab process, Windows Camera process, ADB reverse mapping or
Android camera client. The already-authorized Camera Lab APK remains installed.

Remaining work is an exact synchronized or filmed clock measurement while the
phone points at the Windows clock, followed by longer-running jitter, reconnect
and thermal/load trials. Simultaneous multi-application fan-out and a production
transport/pairing boundary also remain outside this slice.
