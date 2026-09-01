# CAPY-CAMERA-001B1D Local Lab Report

Date: 2026-08-29

Status: sequential two-consumer sharing roundtrip passed; simultaneous
two-consumer diagnostic failed; exact rollback verified; changes uncommitted.

Target: `DESKTOP-AT8EVE9`, Windows NT 10.0.26200.0, x86-64

Scope: `MFVirtualCameraLifetime_Session`, `MFVirtualCameraAccess_CurrentUser`,
`KSCATEGORY_VIDEO_CAMERA`

## Authorized package

- source branch: `codex/capyio-camera`
- source base: `fc3da36`
- media-source CLSID: `{35754be3-54b6-4133-a1c7-1716395c6f1c}`
- built DLL: `target/release/capyio_windows_camera_mf.dll`
- final DLL length: 174080 bytes
- final DLL SHA-256:
  `3DE60DA0F84ACBAFA424EB4E356342D0B8C9B2B81F738AD34CCA09892425CCD8`
- lab deployment path:
  `C:\ProgramData\CapyIO\Lab\Camera\capyio_windows_camera_mf.dll`
- COM registration key:
  `HKLM\SOFTWARE\Classes\CLSID\{35754be3-54b6-4133-a1c7-1716395c6f1c}\InProcServer32`

The closed administrator script verified the exact source hash before copying,
granted Local Service read/execute access by SID `S-1-5-19`, wrote only the
fixed CLSID with `ThreadingModel=Both`, and verified the deployed hash. No
driver, service, privacy, policy, boot or security setting was changed.

## Sequential sharing evidence

The successful elevated `shared-roundtrip` returned exit code 0. One parent
owned the session/current-user camera from start through cleanup and directly
spawned two fixed-argument consumers sequentially. For each consumer, the lab
asserted all of the following:

- its own `MFEnumDeviceSources` call found exactly the parent's camera symbolic
  link using a case-insensitive exact match;
- activation through the enumerated `IMFActivate` succeeded;
- Source Reader returned two real samples within bounded empty-read retries;
- each sample contained at least 1,382,400 bytes for 1280x720 NV12;
- sample duration was exactly 333333 100 ns ticks;
- the second timestamp advanced and its gap was bounded below one second;
- copied CPU-visible luma was in the BT.709 limited range 16..235;
- the activated media source was shut down before child exit.

The first child exited successfully before the second was spawned. A separate
20-second deadline applied to each child. Parent cleanup remained active on
success, child error and timeout paths. The original elevated direct
`roundtrip` was repeated after the sharing check and also returned exit code 0.

Friendly-name-only discovery was not reliable across these process boundaries.
The final lab therefore passes the exact symbolic link returned to the parent
through a bounded child-only environment value. The child rejects values over
4096 UTF-16 units, control characters and non-software-camera prefixes before
enumeration. The raw host-specific link is intentionally not retained here.

## Simultaneous-consumer diagnostic

Before fixing the final sequential gate, the lab was temporarily exercised with
two consumers active at once. A staggered run exposed
`MF_E_INVALID_STATE_TRANSITION` (`0xC00D3E82`) on a repeated source `Start`.
The source and Windows projection were corrected so an active repeated `Start`
retains generation, pending requests, allocator and sample timeline while
publishing the required updated/start events; unit and in-process COM tests now
cover that behavior.

After that correction, simultaneous two-consumer activation still failed with
`MF_E_HW_MFT_FAILED_START_STREAMING` (`0xC00D3704`). This host evidence does not
establish concurrent fan-out and the final `shared-roundtrip` deliberately
validates two independent consumers sequentially instead.

## Exact rollback evidence

The sharing roundtrip and direct roundtrip each stopped and shut down their
session object. The final `cleanup` and `preflight` reported
`existing_registration=false`.

The first administrator removal attempt encountered a transient Frame Server
mapping of the exact deployed DLL. A read-only check then confirmed that the
fixed CLSID was absent and the remaining file still matched the authorized
SHA-256. The same fixed-target removal was retried idempotently and returned
success.

Final read-only verification reported all values below as false:

- deployed DLL exists;
- fixed CLSID exists;
- `C:\ProgramData\CapyIO\Lab\Camera` exists;
- `C:\ProgramData\CapyIO\Lab` exists;
- `C:\ProgramData\CapyIO` exists.

The final non-registering preflight again reported
`existing_registration=false`. Nothing remains deployed or registered by this
lab run.

## Remaining limits

- Sequential process-level reuse is proven; simultaneous multi-consumer fan-out
  is not.
- This does not establish compatibility with named camera applications.
- The host still requires an administrator token for camera start, so no normal
  user product workflow is established.
- Physical-camera capture, remote frame ingress, codecs and Android camera work
  remain outside this slice.
