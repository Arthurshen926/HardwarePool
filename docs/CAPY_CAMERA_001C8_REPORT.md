# CAPY-CAMERA-001C8 Windows Camera application report

- Date: 2026-08-30
- Branch: `codex/capyio-camera`
- Target: `DESKTOP-AT8EVE9`, V2419A / PD2419 over authorized wireless ADB
- Status: Windows Camera displayed live phone-camera pixels; exact rollback complete

## Closed GUI hold command

`capyio-camera-virtual-lab gui-hold` is a fixed, parameter-free lab command. It
starts the existing Session/CurrentUser camera, verifies exactly one symbolic-
link match, prints readiness, holds for 60 seconds, and then always executes the
existing Stop/Shutdown path. It cannot accept a path, CLSID, scope or duration.
The parser contract test covers the new command.

## Ordinary application evidence

The hash-locked media-source DLL remained 190,464 bytes with SHA-256
`5ECDA2958D5F477B72320D0818F731193D776F181BC3EC1F33E9362134904DA9`.
The authorized V2419A run used stream
`bd117f7226a47444b86eb1ce6c9d75a2`, epoch `10530714770275`, and produced
1280x720 AVC at 30 fps. The Windows receiver decoded 2,718 NV12 frames and
published through source sequence 3,040; its first and last frame checksums
were `efd17410f51baccf` and `d2c16f87987ccfb3`.

After `gui_hold_ready=pass` and exact enumeration of one `CapyIO Camera`, the
Windows inbox Camera application was launched fresh. Its visible preview showed
the phone's live workspace scene with normal camera controls. This proves an
ordinary GUI camera application can enumerate, activate, and display valid
pixels from the CapyIO virtual camera. The screenshot was displayed only in the
authorized task conversation and was not written to the repository, preserving
the no-retained-camera-pixels boundary.

The first attempt reached `0xA00F4244 <NoCamerasAreAttached>` because the fixed
hold expired during GUI-tool initialization. Its log showed normal automatic
cleanup. A fresh compact run succeeded; the error was not treated as image-path
evidence.

## Exact rollback

The Camera application was closed, the 60-second hold reported `gui_hold=pass`
and `cleanup=pass`, the Android app was force-stopped, and the exact ADB reverse
mapping was removed. The first administrator Remove invocation cleared the CLSID
but returned 1 while Windows still held the deployed DLL. After the Frame Server
released it, the unchanged symmetric Remove action returned 0.

Final checks reported:

- `existing_registration=false` in the closed preflight;
- no fixed CLSID, deployed DLL, or `C:\ProgramData\CapyIO\Lab` directory;
- no receiver, virtual-lab, or Windows Camera process;
- an empty ADB reverse list and `Active Camera Clients: []` on Android.

No driver, service, privacy setting, boot/security policy, certificate, package,
commit, or remote repository was changed.
