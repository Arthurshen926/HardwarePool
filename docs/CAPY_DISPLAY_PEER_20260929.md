# Display peer workflow — 2026-09-29

Status: manual peer workflow accepted by user on 2026-09-30; automatic quality
is a subsequent update with separate acceptance pending.

The user confirmed sustained picture without problems, Windows stop and layout
restoration, Android initiation/stop, and restarting with changed quality.
These are user-observed physical results; no measured soak duration is claimed.
The previously accepted **legacy** extended desktop/restoration test is not
evidence that this new node-mediated workflow passed.

## First device result and repair

The first two Windows-initiated attempts reached Android's receiver screen but
exited black after about 10 seconds with `CAPY.DISPLAY.RECEIVER_FAILED`.
No decoder-start evidence was captured. This is a failed device test.

Two concrete issues were corrected:

- The initial worker Start ACK acknowledges receipt of configuration, before
  engine/monitor initialization. Service now waits for a lease-bound keyframe
  control ACK processed only after native engine startup before reporting Ready.
- Windows application firewall rules still referenced the old worker directory.
  The new stage has two scoped rules: exact worker EXE, local <LAB_IPV4_04>,
  remote Y700 <LAB_IPV4_01>, TCP 50021 and UDP 50009/50010 only. No broad rule or
  firewall disable was used. Existing rules remain unchanged for rollback.

Repair service stage: `target/integrated-service-staging/display-peer-v2-20260929`,
service SHA256 `FD1F8B8C519CA06935E69E3BC007505458849FFFF32F5AC2B15BB856B70C2C2F`.
Deployment evidence passed. The second device test failed after approximately
10 seconds with `CAPY.DISPLAY.ENGINE_READY_FAILED`; no picture was accepted.
This does not establish a receiver/decoder defect or a 25-second ready timeout.

Diagnostic stage `display-peer-v3-20260929` is deployed, service SHA256
`37352506BD07BA5AD193C5493CF88228CC380C3A0249035020F11A6304DA3892`.
It records bounded worker startup stages and service Ready failure details,
preserves early process exit stages, and distinguishes timeout/pipe/ACK errors.
Build and 14 runtime tests passed; deployment result passed and service display
status was Stopped. Root cause remains unconfirmed pending one instrumented retry.
No change to pairing, driver, or startup timeout was made for this diagnostic.

The instrumented retry failed with service `ENGINE_READY_TIMEOUT`. Worker
remained at stage 44 (OutputBind), not engine start or Android decoding.
Inspection found that ModeMismatch only retried: SudoVDA's preferred mode did
not explicitly replace Windows' persisted mode. The worker now applies the
requested mode to the exact owned target and re-verifies it before capture.
This is a concrete missing step; the prior run did not retain the exact binding
error, so mode mismatch alone is not yet proven to explain that run.

A short physical smoke using the real Sink Capability identity and
1920x1080@60000 mHz passed output binding on DISPLAY12 and exact-GUID removal.
Post-test enumeration matched baseline: DISPLAY22 (0,0,1440,900), DISPLAY39
(1440,0,3360,1080). Ten binding tests and service compilation passed.
Stage `display-peer-v4-20260929` deployed successfully, service SHA256
`BD3C84AE452A0FA648550DA673AD83A529498C819001815E282E59C5F17BE5F2`;
display service status Stopped. End-to-end picture acceptance remains pending.

## Implemented

- Shared node/quality/start/stop UI; 720p/1080p/1440p, 30/60 FPS,
  5/10/15/25/40 Mbps, H.264 SDR; exact receiver capability check, no downgrade.
- Windows Source / Android Sink catalogs and distinct resource consent.
- Existing paired TLS carries one bounded launch/ACK; no separate display invite.
- Service prepares the session, verifies ACK and starts its owned worker; Ready
  precedes Android connection, presented-frame feedback precedes Active.
- Finite Route authorization and Port lease renewal while the session continues.
- Either-end stop and failure/disconnect cleanup, with Windows Stopping retained
  while worker/topology restoration completes.
- Android activity handles orientation/size changes without recreation. No
  Android permission or foreground-service declaration changed in this slice.

## Installed artifacts

- Windows `target/debug/capyio-desktop.exe` SHA256:
  `B0D15729922C5E8EDDDDE6C0028A54B8AA14E2D96D223C05E90746EDB076BCE0`.
- Android `platform/android/app/build/outputs/apk/debug/app-debug.apk`, revision
  `display-peer-session-20260929`, SHA256:
  `2F43DB67788BC09B69EAE71EF6C115825F9214FBD9AABAB4A5FFAFE486008E39`.
  `adb -s <LAB_IPV4_01>:45311 install -r ...` returned Success.
- Service stage `target/integrated-service-staging/display-peer-20260929`;
  `deployment-evidence/result.json` passed. Previous stage is preserved and
  the deployment updater records rollback. No driver or pairing changes.

## Checks

- Protocol: display settings/session tests 5 passed.
- Paired control-link display start/stop loopback test passed.
- Core finite authorization renewal valid/invalid transition test passed.
- Windows display workflow tests 3 passed; service display tests 58 passed,
  2 ignored child-process entrypoints; runtime renewal tests 14 passed.
- Android Rust host tests 13 passed; ARM64/Javac and final APK packaging passed.
- Shared UI typecheck/build and existing display/consent UI tests passed.
- Windows EXE build passed with existing camera unused-code warnings.

## Physical acceptance still required

Keep both applications in the foreground for the first test. Use the original
Windows account and unchanged EXE path; do not launch a second agent-owned UI.
Start in Windows Remote resources -> Display sharing -> Cross-device display,
choose Y700 / 1920x1080 / 60 FPS / 15 Mbps, then allow the Android resource request.
Do not use the collapsed legacy display diagnostics.

1. Android automatically opens a real updating extended desktop; both show the
   negotiated mode. No separate credentials or manual second-side Start.
2. Windows Stop exits Android and restores the original Windows display layout.
3. For Android initiation, first publish Windows's local extended-display
   resource; Android Start prompts Windows consent, then opens the same flow.
4. Android exit stops Windows and restores topology; rotate during reception.
5. Run through renewal, then verify disconnect stops the owned display without
   changing other hardware Routes. Reconnect requires fresh session setup.

This remains a local debug delivery, not signed production APK/installer release.
