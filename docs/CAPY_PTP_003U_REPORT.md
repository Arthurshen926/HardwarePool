# CAPY-PTP-003U Report — Android OEM gesture conflict diagnosis

## Outcome

The v0.8 Android lab now makes a likely phone-side multi-finger interception
visible. An `ACTION_CANCEL` after three or more contacts increments a bounded
counter, explains that the stream was cancelled before Windows could receive
it, and offers a user-initiated route to vivo Super Screenshot settings. The
app writes no setting and adds no permission.

Before the setting change, timestamped logs repeatedly showed the third or
fourth pointer followed within 1--3 ms by `ACTION_CANCEL`. Read-only input state
also identified an OriginOS system-UID full-screen gesture monitor. After the
user disabled the phone's three-finger gestures, the second physical acceptance
processed and submitted all 2,050 frames, repeatedly reached four contacts and
closed cleanly. The user confirmed that both three- and four-finger Windows
gestures then triggered normally.

## Exact evidence

- APK: version 8 / 0.8, SHA-256
  `435915A5869A8089A310B6AE0A1A22A60D483452903372A148AB19302BF2A1B0`,
  arm64-v8a, only `android.permission.INTERNET`;
- receiver SHA-256 used by the accepted run:
  `2E2D8E2DE907976CCFFF2AF552DCCEFB682C678D5945581C320A3DBBBA8A7220`;
- transcript:
  `target/lab-evidence/CAPY-PTP-003U-android-vhf-oem-gestures-disabled-attempt2.txt`,
  SHA-256
  `8C558E080F63538E340CE5539BA58B3665A5B9C4D381FAE5AD533E071755B71A`.

```text
frames_processed=2050
vhf_frames_submitted=2050
max_contacts_observed=4
device_cleanup=closed
CAPY-PTP-003U OEM-gesture-disabled Android-to-VHF multitouch: PASS; max_contacts=4
```

Android assembly and lint passed. Windows was not restarted, and the run did
not install or change the driver.

## Boundary retained

This proves an identified device can deliver stable three-/four-contact streams
after its conflicting OEM gestures are disabled. It does not give a normal app
authority to disable system gestures, nor does it generalize the setting path
to other Android vendors.
