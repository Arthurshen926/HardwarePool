# CAPY-PTP-003Z Report — persistent Android-to-VHF lab session

## Outcome

One Android 1.2 connection remained active across repeated one- through
four-contact input instead of closing after the first completed gesture. The
receiver processed and submitted all 4,356 accepted frames to the installed
VHF device, observed four simultaneous contacts and closed the device cleanly
only after the Android Activity was closed.

The user confirmed that single-finger motion and tapping, two-finger input and
zoom, and three- and four-finger Windows gestures worked during this one
session. Double-tap followed by drag was the only reported interaction that did
not respond. A read-only `SPI_GETTOUCHPADPARAMETERS` query after the run reported
`tapAndDragEnabled=true`, so that gap is retained for a separate timing and
frame-shape diagnosis rather than attributed to a disabled Windows setting.

## Exact evidence

- Android APK: version 12 / 1.2, SHA-256
  `DC4D5015CC04074A003E8150EACB4D712B4BEA6447800E7377F06F4967B2F707`,
  arm64-v8a, only the already approved `android.permission.INTERNET`;
- receiver SHA-256:
  `C13AE3B516EFE267A9F401E880AA4ADE03B7A6E03687FDB26F876DB972D9D4CF`;
- transcript:
  `target/lab-evidence/CAPY-PTP-003Z-android-vhf-persistent.txt`, SHA-256
  `AA80E09AD23A9032B0BED61557E63B6B99AC33EF968699C801EF37B8A0D5562A`.

```text
lab_status=complete
frames_processed=4356
vhf_frames_submitted=4356
max_contacts_observed=4
device_cleanup=closed
CAPY-PTP-003Z persistent Android-to-VHF session: PASS
```

The driver package and Android permissions were not changed, no APK or driver
was installed during this slice, and Windows was not restarted.

## Remaining boundary

This is still an ADB-reverse local lab with a 600-second idle deadline. It does
not establish authenticated production transport, receiver restart recovery or
a persistent least-privilege Windows host. Double-tap-and-drag requires a
separate bounded diagnostic and acceptance.
