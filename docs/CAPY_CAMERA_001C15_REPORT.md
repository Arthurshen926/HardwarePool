# CAPY-CAMERA-001C15 — Read-only live-lab package preflight

Date: 2026-08-30

Status: implementation and current-host read-only verification complete.

## Objective

Turn the C14 release hashes and clean-start assumptions into one repeatable
preflight that cannot deploy, register, start, stop or remove any component.

## Implementation

`scripts/capyio-camera-live-lab-preflight.ps1` accepts no parameters, requires
no administrator token and performs only these checks:

- the fixed release receiver, orchestrator and COM DLL exist and match their
  exact SHA-256 values;
- `capyio-camera-virtual-lab-admin.ps1` pins the same COM DLL hash;
- `C:\ProgramData\CapyIO` and the fixed COM CLSID key are absent;
- TCP port 38173 has no listener;
- the receiver and virtual-camera lab process names are not running.

It deliberately does not call ADB, select a device, deploy/remove files, write
the registry, create a virtual camera, kill a process or retain camera data.

## Evidence

The read-only current-host run reported:

```text
artifact=capyio-avc-lab-receiver.exe sha256=ED32E0A4D7117E07706A6FF04DD5730E17DD2D984EAF06AA1C74AA1ECF89ADD8
artifact=capyio-camera-virtual-lab.exe sha256=4E4F73ADC102FAD15C9E3A24293F4AD34CB0A6C92AC757B86A691E560F5FFF0D
artifact=capyio_windows_camera_mf.dll sha256=F467E22FEA1CA830E7C7F2F7047BA2BB7ECC9F94021784754BBDB95D1294348E
deployment_hash_lock=pass
deployment_state=clean
receiver_port=clean
lab_processes=clean
camera_live_lab_preflight=pass
```

Repository structural validation, documentation validation, `git diff --check`
and the full `cargo xtask ci` passed after this report update.

## Remaining boundary

This proves only exact local artifacts and a clean Windows starting state. A
later physical run still needs a freshly verified ADB endpoint and explicit
authorization for COM deployment, ADB reverse, Android capture,
Session/CurrentUser virtual-camera registration and rollback.
