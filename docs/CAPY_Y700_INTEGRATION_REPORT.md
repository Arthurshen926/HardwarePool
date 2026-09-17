# Lenovo tablet integration checkpoint

Date: 2026-09-17

## Target and authorization

The owner designated a Lenovo Legion Y700 fifth-generation tablet as the new
development target. ADB reports model TB323FU, Android 16, native resolution
1904x3040. Pairing succeeded. No CapyIO package was initially installed.
The owner explicitly approved installing the unified debug APK and testing;
notification, camera and microphone permissions were confirmed through the
Android permission UI. Pairing secrets and transient endpoints are omitted.

## Artifacts

### Unified display bundle import — 0.5.11-dev

The unified APK now opens Android's system document picker from the shared
display card and imports one native-only, bounded connection bundle. The tested
APK has version code 23 and SHA-256
`4A3A5DC7C0B0A4885EC74EBC44FE3543582D70053EEF6BC9DBA85BD070CEE1FB`.
It was installed over the prior unified package on the designated TB323FU.

Physical verification established a cold MainActivity start and system-picker
import. The five exact bundle entries were installed under app-private
`files/capyio-display`; every file is mode 0600, and the public Download copy
was removed. Shared state then reported GameStream configured, native client
available and `canOpen=true`.

The unified Windows service was updated without changing the driver. Its typed
Display control pipe started the schema-3 session, and the unified APK opened
the unexported internal Display Activity through the shared bridge. On TB323FU,
Qualcomm's HEVC decoder rendered the Windows virtual display at 2640x1216 and
120 Hz. Android logged `GameStream video connected; frames presented: 8`; the
retained screenshot records 456 presented frames. Stop removed the worker and
WGC helper, returned MainActivity to the foreground, and left the service
Running with Display Stopped.

An initial incremental Worker artifact stalled before WGC startup. Rebuilding
that target into an empty Cargo target directory fixed the run; the clean Worker
SHA-256 is `B2FD10ED69E9D77B688AD867220DFE3DD6A1B47310A2A6E6BC4946EF01DF46A9`.
This was treated as stale build output, not accepted as a source-code failure.
Evidence is under `target/evidence/integrated-service-y700-20260917-v4/`.

The hardware-free gate covers eight bundle cases, preserving the prior installed
configuration across six invalid archives and replacing CPD1 with GameStream.
The complete optional-native Android gate, Lint, shared UI build, repository
documentation validation and diff check passed. The Windows bundle creator was
also exercised with bounded placeholder inputs; that disposable archive was
inspected and removed, and is not a usable credential bundle.

### Unlocked-device regression after wireless endpoint rotation

On 2026-09-17 the designated TB323FU reconnected at the owner's replacement
wireless-debug endpoint. MainActivity was foreground with the screen awake and
keyguard absent. The existing unified 0.5.10-dev (22) APK was tested without
reinstallation; its previously recorded SHA-256 is
`233BD64E281D116C16EED44B32AF1D0D2C13B34EC7B22DA64973B0F6A2D155C8`.

- Speaker: the two-second native tone helper sent 200 packets / 400 datagrams;
  Android reported Active, 96,000 processed frames, 48 kHz stereo S16LE, no
  problem code. This establishes processing, not subjective listening quality.
- Microphone: the six-second Windows native probe received 489 packets /
  469,440 payload bytes, sequences 0 through 488, with zero malformed,
  wrong-peer, duplicate or discontinuity counters. Android reported Active,
  48 kHz mono S16LE. No recording was retained. The speaker remained Active
  during this test, but no simultaneous tone was sent; this is not a sustained
  full-duplex throughput test or a Windows virtual-microphone acceptance test.
- IMU, gamepad and haptics: shared bridge Start returned true, all three
  actions subsequently reported Active without a problem code, then Stop
  returned true. This is lifecycle evidence only: no remote gamepad/IMU
  consumer or user-confirmed tactile feedback was measured.
- Display: native client available, transport unconfigured,
  `CAPY.DISPLAY.CONFIGURATION_REQUIRED`, Open disabled. No display frames
  were transmitted or rendered.

Final snapshot: all seven local actions Idle. The owned WebView debug forward
was removed. Camera and touchpad media/projection were not exercised in this
run; no Windows service or driver was deployed or changed.

The CDP evaluation helper now rejects protocol errors, absent/unserializable
results, JavaScript exceptions and connections closed before a reply instead
of accepting an absent result as a successful null. Target-selection and
result-parser tests pass (five test groups); normal bridge evaluation was also
repeated on this device after the change.

### Earlier 0.5.7 artifacts

Unified package: `dev.capyio.android`, version 0.5.7-dev (19).

- Initial unconfigured APK SHA-256:
  `1CB62B1678FACC643DCFDBEC62D1126B3251D966B29616A4D4B6E6BDEE1C3840`.
- Audio test configuration APK SHA-256:
  `D15A6C9E8F55B1699F0E9E612096AD24E11D76371A4535AEBA5C2E521BB47D02`.
- Final installed WebView-thread fix APK SHA-256:
  `46CB5E06F66BB2E399DFE092AF886DA05D7864154DF105BEE811282BC64032AF`.

These are dirty-tree local debug builds, not release artifacts. Audio uses
explicit trusted-lab peer metadata and separate test ports, not the existing
Windows system-audio service. No driver was installed or changed.

## Physical evidence

| Capability | Observed result | Limit |
| --- | --- | --- |
| Unified entry | Cold launch opens packaged shared Quick Actions directly | No visual-layout/screen-reader acceptance claim |
| IMU | Accelerometer and gyroscope registered at requested 20,000 us period; service retained connectedDevice foreground ownership after Home | No remote sensor packet or long-soak claim |
| Gamepad | Shared bridge accepts south-button press; snapshot becomes neutral after Home and return while lifecycle remains Active | No Windows game/controller consumer tested |
| Speaker | Three-second tone: 300 packets / 600 datagrams; tablet processes 144,000 frames at 48 kHz stereo S16LE | AudioTrack processing, not subjective sound-quality acceptance |
| Microphone | Six-second receiver window: 497 consecutive packets, 477,120 payload bytes, zero malformed/wrong-peer/duplicate/discontinuity counts; negotiated 48 kHz mono S16LE | Startup overlaps the window; not a loss/latency or system-microphone acceptance test |
| Camera | 60 AVC access units at 1280x720 decoded into 60 NV12 frames / 82,944,000 bytes on Windows; two key frames and one startup discontinuity | ADB-reverse data path; no Windows virtual-camera consumer tested |
| Touchpad | Missing receiver fails explicitly with `CAPY.ANDROID.TOUCHPAD_RECEIVER_UNAVAILABLE` | Windows projection still untested on this tablet |
| Haptics | Lifecycle tested | No user-confirmed tactile/remote-control acceptance |
| Display | Unified APK rendered the Windows virtual display; HEVC, 2640x1216 at 120 Hz, nonzero frames | Lab credentials; no HDR, soak, multi-display or broad-device claim |

Camera inventory contains two Camera2 IDs (front/back), with automatic, 1x
and 2x choices for each. Zoom choices are not six distinct cameras.
No audio recordings or camera frames were saved. Final snapshot shows all seven
local actions Idle, audio and camera Stopped, and no CapyIO foreground service.
Temporary camera reverse and WebView forward mappings were removed; ADB stays
paired for subsequent owner-authorized work.

## Defects and changes

1. Generic APK audio fails with `MIC_CONFIG_MISSING` / `SPEAKER_CONFIG_MISSING`.
   This is a first-run configuration gap, not demonstrated hardware failure.
   The lab build enables validation but does not close ordinary-user onboarding.
2. Camera refresh succeeded internally but raised a WebView exception on the
   JavaBridge thread. `MainActivity.dispatchSharedUiState` now marshals to the
   main Looper and ignores callbacks after Activity destruction/finish.
   On the final installed APK, refresh, source selection and quality selection
   all returned true without the reproduced exception. This physical test
   covers the SDK-bound path that pure Java contract tests cannot execute.
3. WebView page IDs change during Activity recreation. The CDP evaluator now
   optionally discovers exactly one packaged CapyIO page through an explicit
   loopback endpoint, rejecting ambiguous pages and foreign socket origins.
   Three Node tests cover selection, ambiguity and origin rejection.

## Checks and remaining delivery work

`gradlew.bat capyioCheck --offline` passed for the final code, including lint,
APK assembly, entrypoint checks and contract assertions. The audio test package
also passed `verifyIntegratedDebugApk` before the thread-only fix.

Next: replace lab bundle handling with authenticated pairing/enrollment; validate
touchpad/gamepad/IMU against ordinary Windows consumers; add concurrent-route,
reconnect and soak gates; and produce an installer/release package. Display
service/UI/native-library integration now has a physical vertical E2E, but this
report does not mark the complete ordinary-user product deliverable.
