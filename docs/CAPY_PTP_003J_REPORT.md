# CAPY-PTP-003J..003N Report — VHF desktop input acceptance

## Outcome

The installed `CapyIO VHF Precision Touchpad Source` submitted complete contact
snapshots through Microsoft's VHF path and Windows produced observable desktop
input. The controlled local-lab results were:

- `003J`: four one-finger frames moved the system cursor from `(719,585)` to
  `(825,593)`, a measured delta of `(+106,+8)`, without a click;
- `003K`: four tap frames produced exactly one click in an isolated foreground
  target;
- `003L`: seven two-finger frames produced one wheel event with delta `582`;
- `003M`: eleven three-finger horizontal-swipe frames deactivated the isolated
  foreground target and changed the foreground window;
- `003N`: eleven four-finger horizontal-swipe frames deactivated the isolated
  foreground target. The configured Shell consumed the gesture even though the
  target was foreground again at the final sample.

This closes the earlier uncertainty about whether the installed VHF child is a
functional Precision Touchpad input source. It also demonstrates a materially
different result from the user-mode synthetic path: fixed three- and four-finger
frames now reach Windows Shell gesture handling.

## Controlled procedure

All five cases use exact ignored test names compiled into
`touchpad_runtime_worker-1ef346b5542e36f7.exe`, SHA-256
`F7F16EA7F903D1721D2D441B90B5816C19981A94DFA1A242C2C76985F5DD0A7C`.
Each wrapper requires administrator elevation, pins that hash, submits only its
compiled fixture, explicitly releases all contacts and closes the Broker file.
No driver/APK installation, boot-policy change or Windows restart occurs.

The first motion calibration used source timestamps only one microsecond apart,
which quantized HID Scan Time to zero and produced no pointer delta. The accepted
fixture uses 12 ms source intervals; no descriptor or installed driver change was
needed.

Commands:

```text
powershell -File scripts/run_windows_touchpad_003j_vhf_one_finger.ps1
powershell -File scripts/run_windows_touchpad_003k_vhf_tap.ps1
powershell -File scripts/run_windows_touchpad_003l_vhf_two_finger_scroll.ps1
powershell -File scripts/run_windows_touchpad_vhf_shell_gesture.ps1 -Gesture three
powershell -File scripts/run_windows_touchpad_vhf_shell_gesture.ps1 -Gesture four
```

## Evidence

- `target/lab-evidence/CAPY-PTP-003J-vhf-one-finger.txt`;
- `target/lab-evidence/CAPY-PTP-003K-vhf-tap.txt`;
- `target/lab-evidence/CAPY-PTP-003L-vhf-two-finger-scroll.txt`;
- `target/lab-evidence/CAPY-PTP-003M-vhf-three-finger.txt`;
- `target/lab-evidence/CAPY-PTP-003N-vhf-four-finger.txt`.

The current Windows profile does not expose the
`HKCU\...\VirtualDesktops\CurrentVirtualDesktop` value. Therefore `003N` proves
Shell consumption through target deactivation, but does not independently name
the destination virtual-desktop identifier. This limitation does not affect the
observed three-/four-contact recognition or the one-/two-finger results.

## Remaining work

The next slice connects the already installed Android lab Activity to this VHF
Sink through the loopback-only ADB reverse transport. Production work still
requires authenticated transport, a least-privilege privileged Broker boundary,
reconnect/epoch handling, Android foreground lifecycle integration and broader
device/Windows-version qualification.
