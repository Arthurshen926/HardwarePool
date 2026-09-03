# CAPY-PTP-003O — live Android-to-VHF one-finger path

Status: completed

## Goal

Add an explicit VHF mode to the existing loopback-only Android lab receiver and
prove one complete physical phone contact lifecycle reaches the installed VHF
Precision Touchpad without another installation or Windows restart.

## Acceptance criteria

- Synthetic projection remains the default and VHF is explicit opt-in.
- Both existing desktop-input acknowledgement flags remain mandatory.
- Exact Hello binding is accepted before the protected VHF interface opens.
- A physical `0 -> 1 -> 0` contact lifecycle is acknowledged after submission.
- The receiver releases and closes the VHF generation cleanly.
- Format, strict Clippy, package tests and documentation validation pass.

## Result

All criteria passed. The physical gesture processed and submitted 80 VHF frames
with `max_contacts_observed=1`; the device then closed cleanly. No driver/APK
installation, Android permission change or restart occurred.

Evidence: `docs/CAPY_PTP_003O_REPORT.md`.
