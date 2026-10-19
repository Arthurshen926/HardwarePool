# Display retry under Vibepollo, 2026-09-29

User enabled multiple displays in the remote environment. Baseline:
DISPLAY22 Sunshine/spetre at (0,0), 1440x900; DISPLAY39 PHL 278E1 on NVIDIA
RTX 4090 at (1440,0), 1920x1080. Interactive local-user console Session 1 matches
the configured worker session. No driver/configuration/credential changes made.

Two display starts failed. The service initially listened on 7543 and reported
Running, then transitioned to `CAPY.DISPLAY.ENGINE_WORKER_FAILED`. During the
second attempt, worker PID 21396 appeared in Session 1 and then exited. Android
subsequently reported timeout connecting to <LAB_IPV4_04>:7543 with bounded
retries; no presented-frame confirmation was observed. This is not established
as a decoded-black-frame defect. A running service status is not worker readiness.

Deployed worker and engine hashes match launch configuration:
worker BE52A4953D144547F6AA1C01E4C08AD501C6917971702B9195E3A63DE5E135D5;
engine AB1667557181064AC82F30EF3A00347D4EDBC7E57068C6A2D87E36CA78198BDE.
Application event lookup did not reveal a matching worker crash entry.
The worker currently writes its detailed failure to stderr, while its SCM
interactive launch does not preserve that output. Exact failing worker stage
therefore remains unproven; do not attribute it to Vibepollo without evidence.

Stopped display through its existing service control and exited the Android
receiver after the second failure. Original two-screen geometry remains intact.
Next slice: expose a bounded, sanitized worker startup-stage failure through the
existing worker/control boundary, then repair the observed failure. No bypass of
output ownership, authentication or session checks is justified by this result.

## Root cause and recovery

The bounded worker-stage diagnostic identified `ENGINE_START_FAILED` (native
engine InternalFailure). Unlike the prior working display-session directory,
the new service staging directory lacked `assets`, `include`, and `tools`.
The worker launches with its executable directory as its working directory;
the engine requires shader resources/includes and `tools/sunshine_wgc_capture.exe`
there. Restoring these existing matching runtime directories allowed engine
startup, WGC capture, and creation of the additional DISPLAY12 output.

After restarting the failed session, the Android receiver screenshot showed a
real Windows desktop and 3312 presented frames. The user independently confirmed
“有画面了”. Evidence: `target/capyio-display-check.png`. No driver installation,
pairing changes, or remote-desktop configuration changes were used for this fix.
Both lab staging scripts now retain these runtime directories when moving the
worker. The general distribution packager still needs equivalent nested-resource
support; this is not a production release acceptance.

## Physical acceptance

The user then moved a window to the independent extended desktop, checked
updates for approximately one minute, and stopped sharing: “正常，恢复了”.
Post-stop service query confirms generation 3, state `stopped`, problemCode null.
Enumeration confirms the exact baseline geometry restored: DISPLAY22
(0,0,1440,900), DISPLAY39 (1440,0,3360,1080), with no additional display.
No CapyIO display worker remained. The only WGC helper belonged to
`C:\Program Files\Apollo\tools\sunshine_wgc_capture.exe`; it was not stopped
or modified because it belongs to the user's remote-desktop software.

This closes the current-device extended-display / live-image / stop-and-restore
slice. It does not establish a long-duration soak, general installer readiness,
or automatic display provisioning for an arbitrary newly trusted peer.
