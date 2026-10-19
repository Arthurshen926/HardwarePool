# Peer connection status — 2026-09-19

This is a narrow integrated-alpha evidence snapshot, not a completed
peer-to-peer or public-release claim.

## Implemented in this slice

- The Windows shared UI remembers a numeric endpoint only after a native
  authenticated control connection succeeds. Discovery has priority; an
  active paired peer may fall back to the last authenticated endpoint with
  bounded retries. Revoked and unpaired peers cannot use the cache.
- Android control listening prefers TCP 43193 and falls back to an ephemeral
  port on address-in-use. The actual bound port remains visible and announced.
- Android Settings now offers explicit background node waiting independent of
  hardware sharing. It owns the existing visible connected-device Service;
  disabling it does not stop separately active hardware or published Sources.
- Android Remote Resources lists sanitized paired Windows peer metadata and
  distinguishes it from live authenticated control status. Reverse dialing
  is an explicit Settings action, not implicit trust from the peer list.
- New pairing invitations issue role-neutral control leaves (both client and
  server TLS EKUs). Existing pairs are unchanged until the user explicitly
  selects renewal, confirms the interruption, and imports the new invitation
  on Android; the current Windows process must decrypt the old DPAPI bundle.
- Windows now has an opt-in, 20-second single-peer inbound control diagnostic
  (`--verify-paired-control-listener`) that pins the selected peer, exchanges
  catalogs and heartbeats, then closes. It remains separate from the desktop
  app; the Y700 pair was subsequently renewed for dual-role control TLS.
- Windows additionally has an explicitly started persistent inbound listener
  in desktop Settings. A pinned, newly paired Android dialer can enter the
  ordinary live control/catalog worker. Startup is manual and bound to a
  selected local address and peer, not automatic rendezvous. Settings now
  re-reads the native listener owner after WebView refresh instead of relying
  on a page-local button state. The selected paired peer also has a direct
  disconnect action in Settings, so a manually connected peer can release
  its slot before starting the reverse listener even when discovery is absent.
- Android's Rust host paired outbound control/catalog path is now wired to
  JNI, the existing Service control worker and a manual shared-UI entry.
  Requests queue on the worker, load the sole active peer credential from the
  Android store, and report asynchronous failure. This is not automatic
  discovery. The Y700's old pairing lacked dual-role EKU until explicit renewal.
- The Windows Settings page no longer confuses the first-pairing name field
  with the existing paired-device selector.
- Windows Settings now has an explicit current-account pairing-credential
  check. A visible peer record alone is not proof that the process token can
  decrypt its DPAPI bundle; an unreadable bundle produces a concrete account
  recovery message rather than a misleading IP/certificate instruction.
- Windows Settings also offers explicit same-Node pairing renewal. This is a
  manual interruption, not automatic key rotation. The user renewed the Y700
  pair and imported the resulting invitation on the tablet for the physical
  checks below.
- `scripts/build_windows_desktop_debug.ps1` uses Tauri's embedded-UI build.
  Plain `cargo build -p capyio-desktop` produces a development executable that
  tries `127.0.0.1:1420` and is not a standalone user-test artifact.

## Checks and physical evidence

- Desktop TypeScript typecheck, Vite build and eight focused UI tests passed.
- Latest `cargo test -p capyio-desktop --lib`: 87 passed, 10 physical tests
  ignored. Documentation validation and standalone Windows debug build passed.
- `cargo test -p capyio-android-jni control_bind_tests`: fallback test passed.
- Android `:app:assembleDebug` and `:app:verifyIntegratedDebugApk` passed.
  The installed debug APK SHA-256 was
  `5F910EA2F1191610EE4FAA830B0865EFB338A1C0ECB61C5BA9F688F4C34DC52F`.
- The new Android outbound JNI/Service/UI slice passed desktop shared-UI
  typecheck, `cargo test -p capyio-android-jni`, ARM64 `:app:buildRustJni`,
  and `:app:assembleDebug :app:verifyIntegratedDebugApk`. The resulting APK
  has not been installed or physically reverse-dialed in this slice.
- The Windows persistent inbound slice passed desktop Cargo check/tests,
  shared UI typecheck and a synthetic reverse-control loopback with three
  bidirectional heartbeat rounds. It has not passed a physical Y700 dial.
- The integrated debug APK (SHA-256
  `1B5FF5A4757421B3B88B1024A67F0E8E3888F807D932DA66359081BBD8D7C322`)
  was cover-installed on the previously authorized TB323FU without clearing
  app data. Its WebView reported one retained paired Windows Node, a ready
  pairing store, a live control listener on 43193 and the new Android dial
  bridge. Windows TCP reachability to 43193 succeeded. No reverse dial was
  attempted against the old single-role credential. The sandbox process SID
  differs from the user's paired Windows SID; its control CLI cannot decrypt
  the user's DPAPI bundle, which is an execution-account limit rather than a
  TLS or network failure.
- On the authorized Lenovo TB323FU, the updated APK installed successfully.
  Its control listener bound `0.0.0.0:43193` after launch and again after an
  app force-stop/relaunch. Windows TCP reachability to that port was true.
- The rebuilt `target/debug/capyio-desktop.exe` loaded the embedded CapyIO
  interface without a local dev server. The paired-device selector visibly
  showed `Lenovo Y700`; the separate new-device remark field was empty.
- With CapyIO foreground on the tablet, the existing control-only CLI gate
  ran under the actual paired Windows account and reported
  `CAPYIO_CONTROL_VERIFY_PASSED capabilities=2`. This covers pinned mTLS,
  authenticated catalog exchange and one heartbeat without starting hardware.
  Running the same gate under the isolated build account cannot decrypt the
  user's DPAPI credential and is not evidence of a broken peer certificate.
- A subsequent APK update (SHA-256
  `328B2023F4102F1496AA263E13EEADD54980D716CD964868EA53D39DF4F37818`)
  enabled background waiting on the same authorized TB323FU. After Home,
  `dumpsys` reported the Node Service foreground with `connectedDevice` type
  and no Activity binding; the same process and TCP 43193 remained reachable.
  Running the paired control-only gate under the real Windows user succeeded
  with `CAPYIO_CONTROL_VERIFY_PASSED capabilities=2`. Disabling the option
  returned it to non-foreground and the port was unreachable after Home.
  The final APK repeated background TCP and paired mTLS success after adding
  a live-listener check; it was left with the waiting option disabled.
- A later debug APK (SHA-256
  `94B545FB1D4BC8E6B2402F6DEE32A80AA9FCAD9AFDCA1300FE0EA1EE0F635CF7`)
  installed on the same authorized Y700. The Android WebView bridge returned
  one paired Windows node with exactly `nodeId`, `displayName`, `revoked`;
  its Remote Resources screen displayed the node and correctly marked the
  control transport disconnected. It also showed Android discovery as
  broadcasting, not listening. No credential fields entered the bridge.
  `:app:capabilityCompositionTest` passed 294 assertions, the unified APK
  build/structure check and shared UI typecheck passed.
- The authorized user checked that the Windows process could decrypt its
  selected DPAPI credential, then explicitly renewed the Y700 pairing. The
  resulting `.cpi` was imported into the already installed Y700 build; its
  Android paired-record timestamp and pinned peer certificate fingerprint
  matched the new invitation. Windows-to-Android authentication succeeded,
  with the Windows UI reporting synchronized remote resources and the tablet
  reporting one authenticated connection with active catalog exchange.
- The Windows Settings page initially had no disconnect action when the peer
  was manually connected but not discovered. A direct selected-peer disconnect
  action was added and the standalone debug EXE rebuilt. The user then
  disconnected the outgoing link while keeping Windows listening on
  `<LAB_IPV4_04>:43193`. Android-initiated dialing succeeded: Windows TCP
  showed an accepted connection from the tablet, the tablet reported one
  authenticated connection, active catalog exchange, and no control error;
  the state persisted across multiple heartbeat intervals. The user confirmed
  that Windows Remote Resources showed the Y700 and its shared resources.
  This verifies both control/catalog dial directions, not any touchpad or
  media data Route. The temporary `.cpi` copy placed in Android Downloads
  for import was removed; the Windows Downloads original remains for the
  user to delete.

## Not yet proved or implemented

- This run did not complete a physical touchpad Route. The control-only gate
  is not evidence of a working input data channel.
- A Y700 live touchpad run exposed a rapid four-finger failure: the shared
  WebView observed only two `pointerdown` events followed by both pointers'
  `pointercancel` within about 3–35 ms; it did not receive the third or fourth
  pointer. This is upstream of Windows gesture recognition. Separately, the
  bounded shared-UI and Android transport queues previously dropped a new
  non-empty contact topology under pressure, and the Android motion converter
  canceled a snapshot adding more than one contact. Those two source-level
  defects now preserve/rebuild the newest complete snapshot and expand multiple
  additions into ordered pointer-down records; hardware-free tests pass. A
  follow-up APK has bounded Activity-level pointer counts to determine whether
  the observed cancellation starts before or inside WebView. On the same Y700,
  that APK's diagnostic gesture reached four contacts at Activity dispatch but
  only one additional `ACTION_POINTER_DOWN` before cancellation; WebView saw
  two `pointerdown` events followed immediately by `pointercancel`. The tablet
  also reports multiple enabled three-/four-finger system-gesture settings.
  This is consistent with upstream system gesture arbitration, not proof of a
  particular Lenovo gesture. Android requires an app to release contacts on
  `ACTION_CANCEL`; continuing to inject them would risk stuck Windows input.
  The reversible user-controlled system-gesture comparison is recorded below;
  no complete Windows gesture success-rate improvement was claimed at that
  diagnostic step.

The user then disabled the Y700's main three-finger swipe gestures in Android
Settings without ADB mutation. An otherwise identical temporary fullscreen
diagnostic layer recorded one `ACTION_DOWN`, three
`ACTION_POINTER_DOWN`, four WebView `pointerdown` events, four matching releases,
maximum four simultaneous pointers and zero cancellations over about 2.5 s.
This A/B result strongly supports an OEM system-gesture conflict for the
immediate-cancel case. Two separate spread-related settings still read enabled,
so the test does not establish that every three-/four-finger gesture is free of
OS interference. The diagnostic layer was removed after each run. The user
then restarted the real authenticated touchpad Route and confirmed that a
nearly simultaneous multi-finger gesture triggered the expected Windows
action. During that session the Android Activity reached four pointers with
zero cancellations; the integrated touchpad state advanced beyond sequence
1900 and returned to an empty contact set after release. This is one short
physical acceptance sample on the Y700 with conflicting system gestures
disabled, not an unrestricted-device or statistical success-rate claim.

A later repeat with the rebuilt Windows EXE and installed Y700 APK again
triggered the expected Windows four-finger action. Android's bounded native
input counters reached four pointers with zero cancellations. The observed
control socket was Windows ephemeral port to Android 43193, and the touchpad
data socket was Windows ephemeral port to Android's temporary data port. This
repeat therefore confirms a Windows-initiated touchpad Route, **not** the
new listener/dial arbitration on a reverse connection. After continued use,
the Android touchpad state became `failed` with empty contacts while the
control connection remained authenticated and the data socket had closed.
The five-minute single-use consent grant and non-renewing lease in ADR 1472
and the security model are consistent with that transition, but the exact
worker error was not surfaced in the UI, so this is not proof that expiry was
its sole cause. The remote-resource page now provides a prominent stop action
while a Route is active and explains the current five-minute authorization
limit. A separate detail-page bug was found: while waiting for tablet approval,
the Windows view did not poll the accepted Route, so the button could remain
"Apply and use" even after the tablet touchpad became active. The detail view
now refreshes its catalog while open. The Android fullscreen surface reserves
a deliberate stationary five-finger two-second hold to release contacts, stop
the local touchpad Source and leave fullscreen without a visible button. These
UI changes passed build and gesture-helper tests, but still need physical
validation after installing the new APK/replacing the running Windows EXE.
Continuous-use renewal remains future work.

- First-time pairing is still an offline `.cpi` invitation, not a PIN/QR
  confirmation flow. New invitations issue dual-role control leaves, but
  existing asymmetric pairs require explicit re-enrollment.
- Windows and Android can now each initiate a control/catalog connection for
  newly issued dual-role pairs, now physically verified with the renewed Y700
  pair. Windows reverse listening and Android
  reverse dialing still require manual endpoint input. Automatic rendezvous,
  simultaneous-dial arbitration, bilateral
  resource proposals and symmetrical UI authorization remain future slices.
- If an address changes and multicast discovery is unavailable, the user
  still needs to provide a new endpoint. A preferred port is not a rendezvous
  service or overlay-network discovery mechanism.
- Background waiting requires an explicit switch and Android's ongoing
  notification. It is not a boot-time service, nor a guarantee against
  process death, vendor battery policies or network changes. The UI still
  lacks a true whole-node off switch governing every local runtime capability.
