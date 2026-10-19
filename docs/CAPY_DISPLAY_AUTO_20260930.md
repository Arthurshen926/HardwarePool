# Display automatic startup quality — 2026-09-30

Manual peer display was accepted by the user: sustained picture, either-end
start/stop, original desktop restoration and changed manual quality all worked.

The subsequent automatic update is implemented, not yet physically accepted.
The shared UI defaults to automatic and retains manual presets. Android reads
its physical display aspect and refresh rate, probes H.264 decoder candidates,
and returns the final selection over the existing authenticated control Session.
Candidates are landscape, even-pixel, at most 2560x1600 and 60 Hz; lower
same-aspect candidates are tried if required. Bitrate is recommended from pixel
rate within 5–40 Mbps. This is not measured-network ABR or live mode switching.
Windows verifies adapter policy and exact selected capabilities before launch.
Native encoder startup remains a separate physical acceptance step.

Checks: protocol display session/settings 6 passed; Windows display workflow
4 passed; Android automatic-start normalization regression passed; shared UI
typecheck/build and full ARM64/Java/debug APK build passed. Windows tests retain
three pre-existing unused camera helper warnings. The initial APK script call
through Windows PowerShell 5 failed because System.IO.Path.GetRelativePath is
unavailable; running the script in PowerShell 7 succeeded.

APK: platform/android/app/build/outputs/apk/debug/app-debug.apk
SHA256: 85BB03F83675B11BF2A6CC318311079842FEADDACD098F1D62514A354C8888DE
Revision: display-auto-quality-20260930. Local debug only. After the user closed
Windows CapyIO, ADB install -r to Y700 <LAB_IPV4_01>:45311 returned Success.
Windows debug build replaced target/debug/capyio-desktop.exe, SHA256
9AE4E5E037869B9F9B6AC02E2AF44F587847AE7582E88F0B9EEDD19FC135587D.
That initial EXE was incorrectly built without `tauri/custom-protocol` and
attempted to load the absent dev server at 127.0.0.1:1420. The user reported
connection refused. After the user closed it, rebuilding with
`cargo build --offline -p capyio-desktop --bin capyio-desktop --features tauri/custom-protocol`
replaced it with SHA256
05AA18DA6A59DC51B332B69D0CBFE4210827E7A5873DB0C69D598F1914AF85AE.
Build succeeded and the Tauri feature fingerprint includes custom-protocol.
Use the existing integrated package builder (which already enables this
feature) for subsequent deliveries; plain cargo build is not a standalone UI.
No pairing, driver, service or permission changes were made. The original
Windows user must reopen the application; automatic-quality physical acceptance
remains pending, distinct from the previously accepted manual workflow.

Acceptance: start Auto from either end, verify matching final parameters and
correct aspect on both ends; stop/restore; repeat with a manual preset. Confirm
manual remains exact and auto is recomputed on a fresh start rather than during
an active stream.

## Subsequent feedback and update

User reports automatic picture works. Enabling Windows local display publication
made the Windows entry appear on Android, confirming the catalog filter rather
than pairing failure. Android now lists all connected peers and labels those
without a published display Source; Start stays unavailable for those peers.

Changing the active Windows virtual display to 120 Hz returned Android to the
main UI; Windows application remained running. The worker detects mismatch with
the negotiated format. New stage 48 preserves DISPLAY_MODE_CHANGED through
cleanup and concurrent Stop, with an actionable shared UI message. This does not
implement live refresh switching or 120 FPS. UI tests (3), service mapping tests,
native compilation, shared build and complete APK build passed; physical error
message and cleanup regression remain pending.

After user stopped sharing and closed Windows, EXE was rebuilt with
tauri/custom-protocol, APK install -r returned Success (SHA256
F775C1EEFD09DD2F7EFC75E46FF2D5BE73147F02FC33D3FE715B5B5A1B759778), and service
stage display-mode-feedback-20260930 deployment passed (service SHA256
B546316E1377034B4A4B2D06377AD2EB066B90B2D9D834523B0F99CF5CA42E2B).
No driver, pairing or Android permission changes were made.
