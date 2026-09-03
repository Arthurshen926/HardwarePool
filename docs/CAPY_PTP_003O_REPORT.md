# CAPY-PTP-003O Report — live Android-to-VHF one-finger path

## Outcome

The existing Android lab Activity connected through the loopback-only ADB
reverse tunnel to the new explicit `--vhf` receiver mode. After exact Hello
binding validation, the elevated receiver opened the already installed protected
VHF Precision Touchpad interface. One physical phone gesture produced 80 frames,
including the complete `0 -> 1 -> 0` contact lifecycle, and all 80 frames were
acknowledged only after VHF submission. The Sink then closed cleanly.

This is the first direct physical Android-to-installed-VHF proof. It installs no
driver or APK, changes no Android permission and does not restart Windows.

## Exact inputs and gates

- Android endpoint: `100.66.157.119:46143`, model `V2419A`;
- package/activity: `dev.capyio.touchpad.lab/.TouchpadLabActivity`;
- tunnel: device `tcp:61000` to Windows loopback `tcp:61000`;
- receiver SHA-256:
  `3AB868F22805815D3E45AF9D05340823030F6FC3D1A09C9F2F805E9E0532DA69`;
- required receiver gates: `--inject`, `--acknowledge-desktop-input`, and
  explicit `--vhf`;
- single-gesture bound: `--exit-after-release`;
- idle deadline: `--manual-session` selects 600 seconds.

The driver Broker ACL still requires an elevated administrator token. Hello is
validated before that interface is opened, and the listener binds only
`127.0.0.1`.

## Evidence

`target/lab-evidence/CAPY-PTP-003O-android-vhf-live.txt` records:

```text
hello_binding=accepted
device_creation=created
projection=vhf
contact_count_transition=0->1 sequence=1
contact_count_transition=1->0 sequence=79
frames_processed=80
vhf_frames_submitted=80
max_contacts_observed=1
device_cleanup=closed
```

## Remaining work

The next physical acceptance must observe three and four contacts in one live
session and retain the user's Windows Shell observations. Production still needs
authenticated transport, a least-privilege Broker service/process boundary,
Runtime-owned route lifecycle and reconnect/epoch behavior.
