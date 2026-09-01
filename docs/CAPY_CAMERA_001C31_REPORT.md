# CAPY-CAMERA-001C31: Sensor-orientation projection and bounded live hold

## Outcome

C30 reached ordinary Windows Camera with changing phone pixels and retained the
same Android foreground service/Camera2 client across Home and rotation. The
visible frame was 90 degrees counter-clockwise because the AVC record described
only encoded dimensions and discarded the selected Camera2 sensor orientation.
V2419A reports only `android.scaler.availableRotateAndCropModes=[0]`, so the HAL
cannot perform the correction.

C31 advances the private AVC Adapter record to v1.1. Config byte 17 carries a
closed clockwise display rotation code for 0/90/180/270 degrees; bytes 18-19
remain zero. The Rust receiver accepts legacy v1.0 as zero rotation and rejects
unknown new versions, codes or reserved bits. Android publishes the selected
Camera2 sensor orientation. Windows rotates decoded NV12 before shared
publication. A portrait frame is aspect-fitted into the fixed 1280x720 profile
with limited-range black pillarboxes; it is not stretched or silently cropped.

The receiver maximum and closed live-lab argument are also raised from 3,600 to
7,200 access units. At 30 fps this leaves a 240-second receiver bound around the
fixed 180-second GUI hold instead of deterministically ending around 120 seconds.

## Artifacts

- Android C31 debug APK SHA-256:
  `4236E0D99CFBE056ADCF3664CB3BF3238BA25A06B7C2B077FBEF98E559E747F6`
- Windows C31 receiver SHA-256:
  `85C2164E3530F2790AB092D07EE5C7C82C5A106CA3F99A6F71B580703E4F149B`
- Windows C31 virtual-lab SHA-256:
  `6FE25371377680761C3A0F65F0BD5048AF3B796C63D20F1136C4CFABDB846CD8`
- Unchanged C27/C30 COM DLL SHA-256:
  `4C236858C5223B4A1303E825496EBE6799C52E9EAE366DC6DE41C8E9A88F70F0`

## Validation

- AVC v1.1/legacy-v1.0 golden and fail-closed Rust tests: PASS.
- NV12 clockwise rotation, pillarbox and invalid-angle tests: PASS.
- receiver and Media Foundation package all-target tests: PASS.
- Android contract test: PASS.
- isolated offline APK assembly: PASS.
- strict Android lint: PASS.
- full `cargo xtask ci`: PASS.
- C30 Windows and Android temporary deployment rollback: PASS; final read-only
  preflight proved clean ProgramData, CLSID, process and TCP 38173 state.
- authorized C31 APK replacement on V2419A at `100.66.157.119:35491`: PASS;
  package, CAMERA and INTERNET grants were read back after installation.
- ordinary Windows Camera live view: PASS. The selected Camera2 device reported
  `rotation=90`; the phone scene was upright with expected black pillarboxes and
  no stretch or crop.
- stop placeholder: PASS. Stopping Camera Lab removed its foreground service and
  Camera2 client, the receiver entered its 60-second reconnect wait, and the same
  open Windows Camera view showed the deterministic color-bar placeholder rather
  than losing the virtual camera.
- restart transport/capture recovery: PASS. Inside the reconnect window Android
  restored foreground-service type `0x40` and Camera2 device 0, while the same
  receiver accepted `connection=2` with a new stream/epoch and `rotation=90`.
- restart visual recovery: PASS by direct user observation. Real phone pixels
  replaced the placeholder without reopening the Windows Camera consumer. The
  automated evidence still has a limitation: its first post-restart capture hit
  a black transition frame, and later attempts repeatedly returned an unrelated
  browser window despite exact Camera app/title/HWND selection. The pass is
  therefore recorded as witnessed physical evidence rather than an automated
  recovered-pixel screenshot.
- bounded hold and cleanup: PASS. The first and third connected runs ended with
  `live_hold=pass`, `receiver_cleanup=pass` and `cleanup=pass` after 180 seconds;
  the 7,200-unit receiver did not terminate at the former 120-second bound.
- final rollback: PASS using ordinary `Remove`; FrameServer restart was not
  needed. Read-only preflight proved clean ProgramData, fixed CLSID, process and
  TCP 38173 state. Android had no Camera Lab service or active Camera2 client.
  `adb reverse --list` was empty; no camera reverse was created or removed.

## Remaining evidence

No additional physical check is required for the C31 basic-camera slice. A
future test-harness improvement may replace the unreliable Windows window-capture
backend, but that tooling limitation does not block the witnessed functional
result.
