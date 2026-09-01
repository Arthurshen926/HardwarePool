# CAPY-PTP-003T Report — tuned Android multitouch to VHF

## Outcome

The v0.7 Android lab applied the bounded multi-contact motion policy without
changing one-/two-contact coordinates: reaching three contacts rebased fixed
anchors, later movement used 700-per-mille gain until complete release, and the
Activity retained lifecycle while settling MOVE for 24 ms initially and 72 ms
after another pointer joined.

The live Android-to-VHF run passed its transport/device gate. It processed and
acknowledged 2,165 frames, submitted all 2,165 to the installed VHF device,
observed four contacts and closed normally. The user reported that the tuning
made Windows three-/four-finger effects easier to trigger, but that rapid
simultaneous placement was still unreliable and usually required slow, firm
contact.

Follow-up Android and Linux input diagnostics separated that residual behavior
from Windows and from CapyIO's motion scale. OriginOS repeatedly delivered
pointer-down transitions through the third or fourth contact and then delivered
`ACTION_CANCEL` one to three milliseconds later. A representative sequence was:

```text
02:49:13.416 action=DOWN raw=1 effective=1
02:49:13.417 action=POINTER_DOWN raw=2 effective=2
02:49:13.418 action=POINTER_DOWN raw=3 effective=3
02:49:13.419 action=POINTER_DOWN raw=4 effective=4
02:49:13.419 action=CANCEL raw=4 effective=0
```

Read-only `dumpsys input` showed a system-UID full-screen
`global_gesture_monitor` SPY window. The device also reports OriginOS 6 /
Android 16, `three_intercepts_rom_support=1`, the vivo SmartShot settings
Activity, and `persist.vivo.new_three_finger_gesture=true`. Raw touchscreen
sampling independently showed that lightly held contacts can lose tracking IDs
within a few hundred milliseconds. Thus two phone-side effects remain: an OEM
global gesture can cancel an otherwise complete three-/four-contact stream, and
the touchscreen can drop unstable light contacts. Pressure is not present in
the VHF report and is not used by the CapyIO policy.

Windows was not restarted and the installed 0.0.2.0 VHF service remained
running. The run itself installed no driver and changed no phone setting.

## Exact artifacts and evidence

- receiver SHA-256:
  `2E2D8E2DE907976CCFFF2AF552DCCEFB682C678D5945581C320A3DBBBA8A7220`;
- tested v0.7 APK SHA-256:
  `B7811206E1AC8F27C55C20F0A49038F3F23C6153BCE4179A4AA934F28C464F6B`;
- package: `dev.capyio.touchpad.lab`, version 7 / 0.7, only the previously
  approved `android.permission.INTERNET`;
- live evidence:
  `target/lab-evidence/CAPY-PTP-003T-android-vhf-tuned.txt`.

The retained transcript ends with:

```text
frames_processed=2165
vhf_frames_submitted=2165
max_contacts_observed=4
device_cleanup=closed
CAPY-PTP-003T tuned Android-to-VHF multitouch: PASS; max_contacts=4
```

Six capture-policy tests, four mapping tests and four JNI tests passed. Android
arm64 JNI linking, debug APK assembly, lint and installed-hash verification also
passed.

## Remaining work

CAPY-PTP-003U makes probable OEM interception visible in the app and gives the
user an explicit route to vivo's Super Screenshot settings without writing any
device setting. A post-setting live run must show an uncancelled rapid
three-/four-contact stream before further tuning is attributed to CapyIO.
Authenticated non-ADB transport, least-privilege Broker hosting and Runtime
route/reconnect ownership remain production work.
