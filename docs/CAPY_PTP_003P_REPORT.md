# CAPY-PTP-003P Report — live Android-to-VHF multitouch

## Outcome

The installed Android lab Activity connected through the loopback-only ADB
reverse tunnel to the elevated VHF receiver. Exact Hello validation completed
before the already installed protected Precision Touchpad interface was opened.
Physical phone input then reached both three and four active contacts. The
receiver accepted 830 Data frames, acknowledged each only after VHF submission
and closed the VHF generation normally when the Activity exited.

The automated transport/device acceptance passed. This run proves the live
Android capture, private record transport, bounded receiver and installed VHF
projection path through four simultaneous contacts. It does not by itself
identify which Windows Shell destination was displayed. The user subsequently
confirmed that both the live three- and four-finger attempts produced Windows
effects, while also reporting that they were too abrupt and difficult to form.
Fixed `003M` and `003N` fixtures independently prove that Windows Shell consumes
the same three- and four-contact VHF reports.

No driver or APK was installed, no Android permission was changed and Windows
was not restarted.

## Exact inputs and gates

- Android endpoint: `100.66.157.119:46143`, package/activity
  `dev.capyio.touchpad.lab/.TouchpadLabActivity`;
- tunnel: device `tcp:61000` to Windows loopback `tcp:61000`;
- receiver SHA-256:
  `95D05FEC6C24ECF4564CE7FD0301B43C3D542AC5FC784DD4B69B1213B1DC0FA2`;
- required receiver gates: `--inject`, `--acknowledge-desktop-input`, explicit
  `--vhf` and bounded `--manual-session`;
- installed device: the previously approved `003H` 0.0.2.0 package, with the
  `CapyIOVhfTouchpad` service still running after the test.

The Android Activity remained the foreground app and its complete surface was
the best-effort system-gesture exclusion region. Horizontal gestures were used
to avoid the vivo three-finger-down screenshot action. This remains an
app-level mitigation, not a claim that vendor gestures intercepted before
`MotionEvent` can always be suppressed.

## Evidence

`target/lab-evidence/CAPY-PTP-003P-android-vhf-multitouch.txt` records, in part:

```text
hello_binding=accepted
device_creation=created
projection=vhf
contact_count_transition=2->3 sequence=120
contact_count_transition=3->4 sequence=121
contact_count_transition=4->0 sequence=122
contact_count_transition=3->4 sequence=694
contact_count_transition=4->3 sequence=820
contact_count_transition=1->0 sequence=823
frames_processed=830
vhf_frames_submitted=830
max_contacts_observed=4
device_cleanup=closed
CAPY-PTP-003P Android-to-VHF physical multitouch: PASS; max_contacts=4
```

The run began at 2026-08-31 02:22:40 local time and ended at 02:26:01. After
normal protocol close, the ADB reverse mapping and lab process were removed.

## Remaining work

Improve Android multi-contact assembly and motion scaling, then repeat the live
comparison. A separate isolated foreground-target run can strengthen, but is no
longer required for, the user-observed Shell result. Production still needs an
authenticated non-ADB transport, least-privilege Broker hosting, Runtime-owned
route lifecycle and reconnect/epoch integration.
