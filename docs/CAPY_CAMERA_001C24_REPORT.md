# CAPY-CAMERA-001C24 — foreground camera reconnect and cleanup hardening

Date: 2026-08-31

Status: implementation, focused Rust tests, Android offline contract/build/lint,
release build and full repository CI complete. The exact APK and Windows
package were physically exercised under authorization; config, Global mapping,
one virtual-camera enumeration and automatic cleanup passed. Ordinary Windows
Camera pixels were not captured because rotation exposed the C25 lifecycle
defect described in `docs/CAPY_CAMERA_001C25_REPORT.md`.

## Trigger

The first authorized C23 no-ADB run proved the trusted-LAN data path but also
exposed a lab-lifecycle defect. A phone session delivered 358 decoded and
published frames, then the five-second receiver reconnect grace ended while the
Windows Camera consumer still retained the mapping. The parent reported the
earlier receiver-exit error instead of the later mapping-cleanup failure. A
second run then rejected the retained mapping, making a successful data path
look like a transport failure.

The same run showed that the 20-attempt Android start window and 30-second
Windows mapping-start window were too short for an elevated lab launch plus a
human foreground action. These are orchestration bounds, not codec, address or
port failures.

## Outcome

- Android foreground connection attempts increase from 20 to 120. The existing
  500 ms connect timeout and 500 ms retry delay remain fixed, so the initial
  retry window is still bounded to roughly two minutes and stops immediately
  when the visible Activity closes.
- The Activity keeps the display awake only while permission/start/stream state
  is active. `onPause` still closes Camera2, MediaCodec and the exporter; no
  service, background capture or new permission is added.
- Windows waits at most 120 seconds for the first validated Global mapping and
  keeps the receiver alive for a fixed 60-second reconnect grace after a peer
  disconnect. The public lab command still accepts no caller-selected timeout.
- `live-hold` now evaluates child and mapping cleanup before returning a
  validation error. Any cleanup failure is surfaced as `stage=cleanup` and can
  no longer be hidden by an earlier receiver error.
- Camera source labels now distinguish a directly openable Camera2 ID from a
  vendor Zoom target and explicitly state that a Zoom target does not guarantee
  a particular physical lens.

## Camera-selection boundary

The portable selector is intentionally not a physical-lens lock. Every entry
starts with an ID returned by `CameraManager.getCameraIdList()`. Its `@auto`
choice opens that Camera2 device directly; optional minimum/1x/2x entries apply
`CONTROL_ZOOM_RATIO` to the same device and allow the vendor logical camera to
select a lens.

The earlier V2419A C18 run attempted direct physical outputs for advertised
physical IDs and each stalled after one capture callback with no encoded unit.
C19/C20 therefore retained the working logical-camera path. Physical IDs and
focal/sensor metadata remain available in the read-only inventory, but ordinary
UI labels no longer imply exact main/tele/ultra-wide sensor attribution.

## Automated evidence

- `cargo test -p capyio-vcamdroid-adapter --bin
  capyio-avc-lab-receiver` — 6 passed.
- `cargo test -p capyio-windows-camera-mf --bin
  capyio-camera-virtual-lab` — 6 passed, including cleanup-error precedence.
- Offline `contractTest :app:assembleDebug :app:lintDebug` — PASS. The first
  sandboxed build reproduced the documented JDK ZipFS `R.jar` access denial;
  the same network-disabled command passed outside the filesystem sandbox.
- Android `aapt2 dump permissions` — exactly `android.permission.CAMERA` and
  `android.permission.INTERNET`.
- `python scripts/validate_repository.py` — PASS, 84 Requirement IDs.
- `cargo xtask ci` — PASS, including format, check, clippy, workspace tests,
  docs/manifests, Adapter smoke, repository validation and desktop build.
- Release receiver, orchestration executable and unchanged COM DLL built
  successfully. No system registration or physical-device action occurred.
- Parameter-free C24 package/host preflight — PASS: exact hashes pinned and no
  ProgramData root, CLSID, TCP 38173 listener or camera-lab process remained.

## Exact artifacts

- Android debug APK, 2,720,466 bytes:
  `06DB4A6D9D5F4A987C29D70B35CA7CCABF2F218923E9FC3B5DD90687B89048EB`.
- `capyio-avc-lab-receiver.exe`, 249,856 bytes:
  `0A7315F806B249BE9FFCDAEBCF326399AF7B063FF487B19EB6898CA7B54E2967`.
- `capyio-camera-virtual-lab.exe`, 323,072 bytes:
  `7E657283ACB4DFF87B3C3F8F3BE98BDD689898977AF119A4C62BF90A6EFAF1CD`.
- `capyio_windows_camera_mf.dll`, 190,464 bytes, unchanged:
  `0AA8C2F8119059EBF0087E04AC8CBAAAF290C8E641C4535C18E7BC69F6375EE4`.

The current preflight pins the C24 receiver/orchestrator and unchanged DLL.
Historical reports retain the hashes of the packages they actually exercised.

## Physical evidence and cleanup

The authorized C24 APK was installed on `100.66.157.119:46143`; the device-side
base APK hash matched the exact artifact above. No camera `tcp:38173` reverse
mapping was created. A later unrelated `tcp:61000` reverse mapping was left
untouched because it belongs outside this camera slice.

The first non-elevated receiver reached the trusted-LAN config but Windows
rejected `CreateFileMappingW` with error 5. Running the same exact package in
the separately authorized elevated lab context then produced
`live_mapping_ready=pass`, enumerated exactly one `CapyIO Camera`, retained the
fixed 60-second hold and finished `receiver_cleanup=pass` plus `cleanup=pass`.
The Android Activity subsequently returned to stopped state.

Rotating the phone recreated the Activity, invoked its foreground-only pause
path and cleared the deliberately non-persisted endpoint. That prevented the
ordinary Windows Camera screenshot and motivated C25. After the run, Android
was force-stopped, the ProgramData DLL and fixed CLSID were removed, and the
package/host preflight proved no camera listener or lab process remained.

End-user changing-pixel and rotation-continuity evidence therefore belongs to
the new exact C25 APK rather than being claimed for C24.

The trusted-LAN mode remains plaintext lab behavior. C24 does not add CapyIO
pairing, authentication, encryption, Route ownership, a background service or
an installer.
