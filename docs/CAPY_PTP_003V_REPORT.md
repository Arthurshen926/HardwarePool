# CAPY-PTP-003V Report — live Android VHF cursor diagnosis

## Outcome

The live Android-to-VHF path moves the real interactive Windows cursor. The
accepted exact-one-contact run moved the source by `(-1104,-2100)` himetric and
the host cursor from `(960,600)` to `(794,239)`, a `(-166,-361)` pixel delta.
All 155 received frames were submitted to VHF before acknowledgement.

During exact diagnosis, three attempted swipes were raw `1 -> 2 -> 1` Android
gestures. Windows treated them as two-contact input rather than cursor motion.
The accepted fourth gesture stayed at one contact for 18 frames and produced
both source and Windows cursor displacement.

An earlier 633-sample unchanged-cursor window coincided with CapyIO not being
the resumed Android Activity. That window is retained only as invalid test
evidence; it does not explain the reported topology, where UU Remote runs on a
second computer and the phone remains in CapyIO. In that actual topology, the
remaining discrepancy is between the verified Windows cursor coordinates and
UU's remote cursor presentation.

The Android lab now starts its sender only after Hello and initial cancellation
are queued, eliminating an initialization/status race. Version 1.0 also closes
and finishes when it loses the foreground, so switching to a remote-viewing app
cannot leave a misleading connected-but-non-capturing Activity. The receiver's
cursor gate requires VHF, an exact-one-contact gesture and at least 100 himetric
of source motion; two-contact gestures and taps are ignored without ending the
test. It uses `SetCursorPos` only once for the initial measurable anchor and
then records a fresh baseline at each gesture start.

## Exact evidence

- accepted Android APK: version 10 / 1.0, SHA-256
  `84932FE1F8D9CB20D93B428D03CF81CABD64B1AD3CC06705CE3ADDB7E291148B`,
  arm64-v8a, only `android.permission.INTERNET`;
- receiver SHA-256:
  `65C96C37EB14513E08C55116EA9B52DFB18AE21B7E137218E90BD2652B6C820B`;
- accepted transcript:
  `target/lab-evidence/CAPY-PTP-003V-android-vhf-cursor-attempt8.txt`, SHA-256
  `72D10588DF74D3A6972D0011057AF513C14003E5A75E1292C76B253012DE0077`;
- separate interactive cursor visibility probe:
  `target/lab-evidence/CAPY-PTP-003W-cursor-visibility-probe.txt`;
- invalid foreground sampler retained for audit:
  `target/lab-evidence/CAPY-PTP-003Y-host-cursor-sampler.txt`.

```text
frames_processed=155
vhf_frames_submitted=155
accepted_exit_gesture_peak_contacts=1
single_contact_frames_observed=18
single_contact_source_delta=timestamp_nanos:319484702,x_himetric:-1104,y_himetric:-2100
cursor_before=960,600
cursor_after=794,239
cursor_delta=-166,-361
cursor_moved=true
CAPY-PTP-003V live Android VHF cursor: PASS
```

Four receiver CLI/motion-gate tests, Rust formatting and Clippy passed. Android
assembly/lint and installed-APK hash verification passed. The installed
`CapyIOVhfTouchpad` 0.0.2.0 service remained `RUNNING`; no driver change or
Windows restart occurred.

## Remaining work

CAPY-PTP-003Z separates the one-shot diagnostic from a bounded continuous lab
session and makes the Android status explain that a Windows receiver must also
be running. Authenticated non-ADB transport, reconnect/epoch ownership and a
least-privilege persistent Windows host remain production work. Supporting a
UU Remote view now requires a separate compatibility comparison because UU
uses its own virtual display and mouse/cursor channel; host cursor motion alone
does not prove what the remote client renders.
