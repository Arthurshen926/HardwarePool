# CAPY-CAMERA-001C7 registered live-camera report

- Date: 2026-08-30
- Branch: `codex/capyio-camera`
- Base commit: `fc3da3636ca6c969667e71a9b596dcc944380146`
- Target: `DESKTOP-AT8EVE9`, V2419A / PD2419 over authorized wireless ADB
- Status: registered Session/CurrentUser live-camera roundtrip and exact rollback complete

## Failure found and fixed

The first controlled Global/registered run reached COM deployment, camera
enumeration and Frame Server activation, but the first Source Reader request
failed with `MF_E_NOTACCEPTING` (`0xC00D36B5`). The Global receiver was healthy
and had published 1,669 decoded frames. The defect was in the media source:
Frame Server can issue another `RequestSample` before a 30 fps producer commits
the next frame, and the synchronous shared provider treated that normal interval
as a fatal method error.

The registered shared provider now reserves at most four requests and posts
them to a Media Foundation serial work queue. Tokens remain FIFO. When no newer
publication exists, a scheduled work item retries after 5 ms without blocking
the callback or emitting an error. Stop/shutdown or a real asynchronous failure
releases every bounded reservation. Fixture and process-local ingress behavior
remain unchanged.

Microsoft documents that non-state errors returned by `RequestSample` are fatal
to the pipeline, and that delayed Media Foundation work items use a negative
millisecond interval:

- <https://learn.microsoft.com/en-us/windows/win32/api/mfidl/nf-mfidl-imfmediastream-requestsample>
- <https://learn.microsoft.com/en-us/windows/win32/api/mfapi/nf-mfapi-mfscheduleworkitem>

The regression test consumes one shared publication, submits another request
while no newer frame exists, waits 20 ms, publishes a second distinct frame and
observes that exact luma value in the eventual `MEMediaSample` event.

## Authorized package and operation

- media-source DLL: `target/release/capyio_windows_camera_mf.dll`
- length: 190,464 bytes
- SHA-256: `5ECDA2958D5F477B72320D0818F731193D776F181BC3EC1F33E9362134904DA9`
- fixed CLSID: `{35754be3-54b6-4133-a1c7-1716395c6f1c}`
- deployment: `C:\ProgramData\CapyIO\Lab\Camera\capyio_windows_camera_mf.dll`
- scope: `MFVirtualCameraLifetime_Session`, `MFVirtualCameraAccess_CurrentUser`

The closed administrator script verified the source hash, copied only that DLL,
granted LocalService read/execute access and wrote only the fixed COM CLSID.
No driver, service, privacy toggle, boot setting, certificate or Windows
security policy changed.

## Successful physical roundtrip

The exact `adb reverse tcp:38173 tcp:38173` tunnel carried V2419A Camera2 /
MediaCodec Annex-B AVC to the elevated Global receiver. The successful run used
stream `ef2dc243a7c6d37fe8cb9a8cdd69521e`, epoch `9415593685824` and published
1,226 decoded 1280x720 NV12 frames through source sequence 1,382. Receiver
first/last checksums were `360886a8ea816b5c` and `298f449319dd3176`.

The elevated closed `roundtrip` then reported:

```text
roundtrip=pass
enumerated_matches=1
sample_bytes=1382400
sample_duration_100ns=333333
sample_delta_100ns=666610
first_luma=154
cleanup=pass
```

This proves the Android camera stream reaches the Windows Global camera-host
mapping, registered activation selects that mapping, Frame Server activates the
CapyIO media source, and a system-enumerated `CapyIO Camera` supplies advancing
NV12 samples to a Source Reader. It does not yet prove compatibility with a
named ordinary GUI camera application or simultaneous multi-application fan-out.

## Exact rollback

The roundtrip stopped and shut down its Session camera. Cleanup then force-
stopped the Android app, removed the ADB reverse mapping and left Android Camera
Service at `Active Camera Clients: []`. The receiver exited with no remaining
process. The administrator Remove action deleted the fixed CLSID, deployed DLL
and empty CapyIO lab directories.

Final read-only checks reported all of the following absent:

- Session/CurrentUser virtual-camera registration;
- `C:\ProgramData\CapyIO`;
- deployed media-source DLL;
- fixed HKLM CLSID;
- receiver and virtual-lab processes.
