# CAPY-CAMERA-001B1C Local Lab Report

Date: 2026-08-29

Status: Frame Server roundtrip passed; exact rollback verified; changes
uncommitted.

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
  `FA1389606818D4753F064B66F1B484C73D059E8E91DE36EF11258EDDCAC355BE`
- lab deployment path:
  `C:\ProgramData\CapyIO\Lab\Camera\capyio_windows_camera_mf.dll`
- COM registration key:
  `HKLM\SOFTWARE\Classes\CLSID\{35754be3-54b6-4133-a1c7-1716395c6f1c}\InProcServer32`

The closed administrator script verified the build-tree hash before copying,
granted Local Service read/execute access using SID `S-1-5-19`, wrote only the
fixed CLSID with `ThreadingModel=Both`, and verified the deployed hash. The
observed deployed values matched the package above.

After the first qualifying run, two source-only Clippy corrections changed the
release hash. The final hash above was therefore deployed and the complete
elevated roundtrip and rollback were repeated instead of inheriting evidence
from the earlier binary.

## Roundtrip evidence

The successful elevated `roundtrip` returned exit code 0 and performed all of
the following assertions before reporting success:

- `IMFVirtualCamera::Start` completed for the session/current-user plan;
- `MFEnumDeviceSources` returned exactly one case-insensitive symbolic-link
  match with friendly name `CapyIO Camera`;
- `IMFVirtualCamera::GetMediaSource` succeeded;
- a synchronous Media Foundation Source Reader returned two actual samples,
  skipping only bounded empty live-source reads;
- the first sample contained at least 1,382,400 bytes for 1280x720 NV12;
- sample duration was exactly 333333 100 ns ticks;
- the second sample timestamp was greater than the first and the observed gap
  was less than one second; the in-process source test separately proves the
  underlying deterministic samples are exactly 333333 ticks apart;
- the first sample was copied into a bounded CPU memory buffer and its first Y
  byte was in the BT.709 limited range 16..235;
- registrar stop and shutdown completed after validation.

Frame Server event evidence also showed successful shared-source
initialization with one stream and successful NV12 1280x720@30 output-type
selection. Earlier diagnostics established that direct manual event polling is
not a valid substitute for Source Reader semantics: successful live-source
reads may initially return no sample, and downstream buffers need not implement
`IMF2DBuffer` or retain the source's exact timestamp spacing.

On this host, a non-elevated `Start` returned `0x80070005` even though Windows
camera access, app camera access and desktop-app camera access were all already
enabled. The same session/current-user plan passed under an administrator
token. No privacy, policy, boot or security setting was changed.

## Exact rollback evidence

The roundtrip itself stopped and shut down the session object. A subsequent
`cleanup` and `preflight` both reported `existing_registration=false`.
Administrator removal then deleted only the fixed CLSID and hash-recorded DLL.
An earlier qualifying run encountered a transient post-Frame-Server DLL
mapping and succeeded on an idempotent retry. Removal of the final hash-recorded
artifact returned success on the first attempt.

Final read-only verification reported all values below as false:

- deployed DLL exists;
- fixed CLSID exists;
- `C:\ProgramData\CapyIO\Lab\Camera` exists;
- `C:\ProgramData\CapyIO\Lab` exists;
- `C:\ProgramData\CapyIO` exists.

The final non-registering preflight again reported
`existing_registration=false`. No driver package, service, boot setting,
Secure Boot setting, certificate, privacy setting or unrelated COM
registration was changed.

## Remaining limits

- This proves Media Foundation/Frame Server activation and deterministic frame
  delivery on the recorded host, not compatibility with every camera app.
- The host-specific administrator-token requirement needs a separate product
  decision before this can become a normal user workflow.
- Physical-camera capture, transport codecs and an Android camera Adapter are
  outside this slice.
