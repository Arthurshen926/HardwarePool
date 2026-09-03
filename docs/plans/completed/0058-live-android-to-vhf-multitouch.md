# CAPY-PTP-003P — live Android-to-VHF multitouch acceptance

Status: completed

## Goal

Observe physical three- and four-finger horizontal gestures from the Android
lab Activity in one bounded VHF session and close the connection normally.

## Acceptance result

- The inspected v0.6 lab APK connected over loopback ADB reverse.
- Exact Hello validation completed before the installed VHF Sink opened.
- The receiver processed 830 frames, reached both three and four contacts and
  acknowledged each accepted frame only after VHF submission.
- Normal Activity close released the connection and VHF generation.
- The user confirmed that both live three- and four-finger attempts produced
  Windows effects; the same observation identified excessive speed and
  difficult contact formation as the next Android policy issue.
- No installation, permission change or Windows restart occurred in this run.

Evidence:
`target/lab-evidence/CAPY-PTP-003P-android-vhf-multitouch.txt`.

Detailed report: `docs/CAPY_PTP_003P_REPORT.md`.
