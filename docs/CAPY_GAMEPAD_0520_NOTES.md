# Gamepad 0.5.20: rotation and projection selection

## Implemented

- Unified MainActivity handles orientation and display-size changes without
  recreating its WebView. Fullscreen navigation and layout remain in memory.
  Rotation releases old-size input and updates gesture exclusion/insets; normal
  pause/stop cleanup remains unchanged. No permission or service declaration changed.
- The touch surface clears pointer ownership and pending controls on resize.
- DSU is an optional collapsed compatibility section. DS4 carries motion itself.
  Both UI and host reject starting DSU while DS4 is active, preventing DSU from
  replacing the single projection destination. DS4 already rejects active DSU.
- UI no longer describes the whole DS4 experience as complete based on earlier
  input-report evidence.

## Still required for the requested switchable controller

- Actual game feedback is not yet forwarded to Android. DS4's low-level seven-byte
  feedback decoder and Android's vibrator sink exist, but a separately authorized
  reverse haptics Route is still needed. Local tap vibration is not game rumble.
- Xbox selection currently does not start a standalone Xbox projection. The
  optional ViGEm helper was designed as a DS4 companion, not a mode switch. Its
  executable/client DLL were not found beside the current debug executable.
- Standard XInput has no native motion fields; an optional motion-to-stick
  converter must be explicit, not described as native Xbox gyro.
- Updated-app DS4 gyro, game rumble and physical rotation acceptance must be
  reported separately. Build/fixture results do not prove those interactions.

## Evidence (2026-09-28)

- `pnpm --filter @capyio/desktop build`: passed typecheck and Vite build.
- `cargo test --offline -p capyio-desktop --lib gamepad -- --test-threads=1`:
  22 passed, one physical test ignored; includes the DSU non-replacement case.
- Android `:app:capabilityCompositionTest :app:assembleDebug --offline`:
  319 contract assertions passed and APK built.
- Desktop custom-protocol build passed; replaced `target/debug/capyio-desktop.exe`
  after user confirmed closure. Existing camera dead-code warnings remain.
- Authorized overwrite install to Y700 `<LAB_IPV4_01>:38121` succeeded.
  Package readback confirms versionCode 32 / 0.5.20-dev. MainActivity resumed
  and the shared hardware page rendered. Physical rotation and game feedback
  have not been validated in this slice. No ADB forwarding/reverse used.
- Windows registry contains a ViGEmBus driver entry. This is not proof of a
  working Xbox session. Older sidecar artifacts exist in the tools directory,
  but the default reviewed package path is missing and nothing was installed.

## Follow-up: reported missing DS4 motion

Read-only native HID observation of the current 054c:09cc device received
26,889 64-byte reports in 15 seconds. DS4 USB report 0x01 gyro axes ranged
[-1,1], [-1,0], [-1,1]; acceleration axes ranged [27,34], [3582,3592],
[3578,3585] in raw report units. This proves non-neutral, varying motion fields
reach the Windows HID interface, not correct response to deliberate rotation:
the user has not yet confirmed performing the requested motion during capture.

The probe now separates motion-axis ranges from generic changed-report counts;
DS4 packet counters alone make the old generic count unsuitable as gyro proof.
Shared UI source adds received sensor values beside DS4 status and explains
native gyro versus stick mapping. Typecheck/build passed; that UI follow-up has
not been deployed over the running desktop executable or installed APK.

The user's tester is https://www.9slab.com/gamepad/home. Browser automation was
unavailable and its dynamic implementation was not verified. Do not assert it
does/does not use WebHID. The standard Gamepad API does not define native gyro
fields (https://w3c.github.io/gamepad/); a normal axes-only display cannot prove
native DS4 motion absence. Actual application compatibility remains unresolved.
