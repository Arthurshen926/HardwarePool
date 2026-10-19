# Integrated delivery follow-up — 2026-09-29

## Scope and acceptance still pending

1. Current Y700 extended display: real frames, sustained operation, explicit Stop
   and restoration of the pre-start desktop topology.
2. Microphone, speaker and camera: either peer initiates; existing consent and
   authenticated control connection coordinate the existing media transports;
   stopping releases both endpoints. Do not label compatibility starts as live
   Runtime Route integration.
3. Concurrent use, complete packaging, upgrade and actionable failures.

## Completed in this follow-up

- Found the installed Android APK missing the optional GameStream native client.
  Its native state reported `CAPY.DISPLAY.NATIVE_CLIENT_MISSING`.
- Added `scripts/build_integrated_android_debug.ps1`. The complete debug build
  requires the existing verified GameStream stage and runs both integrated APK
  and GameStream APK verification. It neither installs nor enables Release.
- Built and installed the complete debug APK on TB323FU at
  `<LAB_IPV4_01>:45311`; installation returned Success. SHA256:
  `53D95180F7E82335DB43F42FEFDC440EF777FF69378934B985A2B1089A89A650`.
- Native readback after restart: configurationPresent=true,
  nativeClientAvailable=true, canOpen=true, problemCode empty. This is
  prerequisite evidence, **not decoded-frame evidence**.
- Windows UI again showed authenticated Y700 connection and synchronized
  catalog, without replacing pairing credentials.
- Existing display control CLI returned stopped/generation 0/no problem.
  No display Start was issued in this follow-up yet.

## Physical validation baseline and interruption

Read-only display enumeration found one active GameViewer virtual display:
`DISPLAY2`, rectangle 0,0–1440,900. SudoVDA is installed but no active SudoVDA
output was present. Do not suppress or remove the GameViewer output.

The Windows automation screenshot intermittently showed unrelated foreground
applications while accessibility still described CapyIO. Foreground testing was
paused and a short user-controlled desktop test window requested. No successful
extension, frame-stability or restoration result is claimed yet.

The service's display-control SID is the older test account ending in 1004,
whereas the user's SID ends in 1001. However, the pipe also allows administrators;
this mismatch alone does **not** establish an administrator-window failure.
No service configuration or driver was modified.

## Media integration boundary

The existing native audio Route binding and microphone supervisor can be
reused. Windows service audio currently takes administrator-configured static
endpoints. Dynamic Route endpoint control conflicts with the current normative
security/architecture restrictions on changing addresses through IPC. The
affected implementation is paused pending the proposed ADR; existing audio
continues unchanged. Camera also still needs coordinated remote startup and
frame-readiness acknowledgement, not a replacement codec/transport.

The complete APK remains local-debug-only. Signed distribution, installation,
upgrade, concurrent performance and all three acceptance groups remain open.

## Follow-up after explicit user confirmation

The user accepted ADR 1501's scoped boundary and the desktop test window.
The isolated audio Route service implementation is proceeding; this does not
authorize widening the existing interactive-user pipe.

The first display run completed launch and HEVC decoder configuration but
showed a black Surface with no presented-frame count. Stop returned the service
to stopped. Its baseline was GameViewer DISPLAY2, whereas the observed final
topology was physical PHL 278E1 DISPLAY39. Whether the remote-display manager
was changed concurrently has been asked; that first restoration is not passed.

Two Android receiver defects were corrected: drain decoded output independently
of new input (a delayed final/static frame otherwise remains queued), and make
the Surface visible immediately after its first size update. Complete APK
verification passed and the replacement was installed successfully. SHA256:
`1235283BA0112B8F926B581D80CC25EE908F839C84EEF989FE1DE28A393FCBA7`.

Second physical run, using the PHL 278E1 baseline:

- Existing PHL DISPLAY39 remained at 0,0–1920,1080.
- SudoVDA CapyIO-000000 DISPLAY12 became active at
  3840,0–6480,1216 (independent 2640x1216 extended output).
- Android logged its first presented frame at 16:09:34 local device time;
  screenshot counters subsequently increased to 338 and 1865.
- Screenshots remained black despite the increasing counter. A window-content
  check was requested. These counters prove output submission, not correct
  desktop content, sustained 120 fps, latency or visual stability.
- Explicit Stop reached stopped/generation 2/no problem. The worker exited,
  the CapyIO display disappeared, and PHL DISPLAY39 remained at the same bounds.
  Restoration passed for this second baseline only.
- Android was returned to the unified app with Back. No driver was installed,
  no display was manually suppressed, and no service configuration was changed.

Remaining display blocker: visible desktop-content validation/black-frame
diagnosis. The display feature is not yet accepted as complete.

## Accepted audio service slice

ADR 1501 is accepted following the user's explicit confirmation. The service
now has a separate opt-in SID-scoped audio Route pipe, enabled only with
`--audio-route-control-user-sid`; the legacy pipe and configured endpoints
remain unchanged. Microphone and speaker use independent dynamic Route workers,
15-second local-owner renewal and the original finite grant deadline. A legacy
operation cannot mutate a Route-owned worker.

Checks: three audio Route service tests, eight dynamic audio binding tests,
Windows service all-targets check and document validation (88 referenced IDs)
passed. No external production dependency was added.

This code is not deployed. Desktop/Android orchestration, the explicit stereo
offer extension and physical audio validation remain to be connected. Existing
UDP transport is unchanged and is not claimed to provide application encryption.

Windows UI automation could not reactivate the captured CapyIO window on two
attempts (`failed to activate captured window`), so further window manipulation
was stopped. Display service is stopped and the temporary display removed.

## Subsequent integration and deployment status

The display control account mismatch was reproduced in the actual desktop UI:
the service allowed the sandbox SID rather than the user's SID ending in 1001.
The approved account repair completed and its result is recorded under
`target/evidence/display-account-repair-20260929/result.json`. The original
account could then query display status and start it from the UI. Pairing and
drivers were not changed.

A later display run again showed black output while the active topology changed
to GameViewer DISPLAY2. This does not establish who changed that topology.
The new worker checks whether its owned output remains available and exits
through cleanup after sustained loss; the runtime preserves the specific worker
failure and the UI explains the display-manager conflict. Service display
runtime tests (11) and display UI tests (3) passed. The new worker is built,
but is not yet deployed at the time of this entry.

Audio orchestration is implemented on both ends, including direction-specific
ownership, finite authorization, mono/stereo offers, and real frame progress
before reporting active. The complete audio APK was installed once; a later
per-direction status correction still needs installation. The Windows frontend
production build passed. Rust compilation/linking completed, but final EXE
replacement was denied because the original-path desktop process (PID 104872)
was still running. No successful replacement or physical audio acceptance is
claimed from that build attempt.

The matching service deployment is awaiting interactive UAC confirmation. Only
the old `0.5.11` service stage was present when checked. Camera Route integration
is still in progress. Local-debug package verification and reversible directory
upgrade scripts are implemented and exercised with temporary fixture files;
no actual complete package or production installer has been delivered.

Deployment subsequently completed at 09:10 UTC. The result in
`target/integrated-service-staging/route-20260929-090958-5904/deployment-evidence/result.json`
records `passed: true`, `Running`, and both display/audio Route control SIDs
ending in 1001. Service SHA-256:
`9ACE6F72D9437EDFD1BDC71A4FBFDD699FC3F2F3D30F85C7DFE6F94569386BE2`.
This supersedes the pending service deployment above, not the pending actual
media acceptance or desktop/APK replacement.

## One-click speaker physical playback follow-up

The matching desktop and complete APK were subsequently replaced. Current
installed APK SHA-256 is
`4A10FBBD5579FED854170E0885506FA5C49E2FFE0C3808694D08BCF9FB69734C`.
Physical UI runs found and corrected the old media command guard, filtering of
non-demo Routes from the desktop snapshot, Android consent-status handoff, and
late Draft acknowledgements overwriting the negotiated audio offer. Failure
diagnostics now survive cleanup. Silence after speaker preparation no longer
counts as hardware failure; actual received media is still required for Active.

The user then started the speaker from the Windows remote resource entry and
played through CapyIO Speaker. The user explicitly confirmed audible sound on
the tablet and a running Windows status. Read-only Android observation agreed:
speaker Route Active, `shouldRun=true`, `waitingMedia=false`, 2,143,200 processed
frames, 48,000 Hz, stereo PCM S16LE, and no problem code. The microphone remained
Stopped with zero frames. Node/media traffic used Tailscale, not an ADB tunnel;
the loopback WebView forward was used only to read UI/runtime diagnostics.

This passes the short one-click speaker playback case, not stop/restart,
long-duration, concurrency, microphone/camera, or display acceptance. The user
has been asked to stop the speaker before starting the microphone case.

The subsequent reported busy/no-audio case was not a microphone run: Android
UI inspection showed the legacy local speaker Quick Action page, with generation
3 Active and zero frames; microphone generation remained zero and Stopped. A
new speaker Route was Failed while that independent local player remained
Active. This is not evidence of a successful reverse Route or a microphone
failure. The user has been asked to stop that player and close the desktop for
the matching update. The local media detail entry needs to lead to the same
peer-aware flow instead of the legacy start button.

## Latest acceptance: pairing and bilateral speaker

The Android microphone/speaker detail entry now uses the peer-aware AudioUsePanel;
legacy local worker controls are folded into advanced diagnostics. The repeated
pairing discrepancy was traced to an agent package-local AppData file view, not
the user's chosen executable or account. An invitation generated by the user's
normally launched window restored authentication. The user confirmed that a
normal close/relaunch still connects. See `CAPY_PAIRING_FILE_VIEW_20260929.md`.

The subsequent Android-initiated speaker failure was a distinct finite-grant
sequencing bug: Android granted eight hours before receiving Windows' one-hour
approval. The Android intent now waits for that finite approval and bounds its
own grant accordingly; strict reciprocal-offer checks remain unchanged.
Revision `audio-peer-grant-20260929` was built and installed, with no Windows
EXE/service/driver or pairing change. See `CAPY_AUDIO_PEER_GRANT_20260929.md`.

The user confirmed Android-initiated audible playback and stopping at any time.
Together with the earlier Windows-initiated audible playback, this closes the
reported bilateral speaker start/stop defect. It does not certify long-duration
or concurrent-feature use. Microphone/camera bilateral one-click validation and
visible display acceptance remain open.

## Microphone one-click follow-up

After speaker acceptance, stopped the active speaker via Android's normal
`stopAudioUse` handler and initiated microphone via `startAudioUse`. Windows
requested resource consent; the user allowed it and confirmed Windows reports
running without separately starting its receiver.
Android Route `<LAB_ID_13>` reports Active, no error,
and 1,843,200 captured frames at 48 kHz mono PCM S16LE. Speaker reports stopped.
This establishes Android initiation/capture and the user's Windows running-state
observation, not yet Windows application-level sound or stop acceptance. The
user has been asked to check the selected virtual microphone's input meter and
then stop from Windows. No recording file is created by this test.

The user subsequently confirmed the microphone verification has no problems.
This records user acceptance of the requested input-meter and stop check. Before
the next camera-only test, a newly active microphone Route was observed and
stopped through the normal Android handler to avoid accidental concurrent capture.
No long-duration/concurrent-media claim is inferred from this short check.

## Camera one-click follow-up

Android initiated camera Route `<LAB_ID_04>` through
the normal UI handler, then Windows resource consent was requested. Android
subsequently reported Active with no error. The user confirmed the requested
Windows Camera live-image and Windows-side Stop verification had no problems.
A subsequent readback shows camera Idle/shouldRun=false and both audio directions
Idle. This accepts the short camera image/stop case, not long-duration stability,
all initiation paths, or concurrent media. No photos or recordings were saved.

## Short concurrent speaker/camera check

Started speaker `<LAB_ID_14>` and camera
`<LAB_ID_06>` via normal Android UI handlers.
Both reached Active without errors. The user reported no notable problems or
obvious lag during the requested approximately one-minute playback/preview and
page-switching check. This is subjective short-run acceptance, not a latency
benchmark or long-duration soak.

A background Activity diagnostic returned empty/idle defaults after its service
binding was released. OS service inspection confirmed both foreground services
still running; bringing MainActivity forward restored both Active observations.
Do not mistake the unbound WebView fallback for an actual media stop. That
diagnostic ambiguity is not repaired by this evidence entry.

Stopped only camera from Android: camera became Idle/shouldRun=false while the
same speaker Route remained Active/shouldRun=true. User audible continuity and
final speaker Stop confirmation are pending. Display visible-image/topology
acceptance remains the next uncompleted hardware check.
