# CAPY-INTEGRATED-ALPHA-001 report

Date: 2026-09-04

Branch: `codex/capyio-integrated-alpha`

## Result

The bidirectional-audio baseline and portable contracts now share one local
integration line with camera, Precision Touchpad and touch+motion gamepad
prototypes. The Windows Tauri host compiles with audio Quick Actions, live IMU,
the Controller surface and gamepad projection ownership together. Touchpad and
camera platform crates remain available without importing either into Core.

The first cross-platform UI integration slice upgrades the Browser Mock and
Tauri host to `UiSnapshot` schema 5. The same Vue/TypeScript component displays
the Runtime's local and peer Capability catalogs, including the newly catalogued
touchpad Source/Sink. Only a local Capability with at least one Source Port can
be selected for sharing. Peer entries and local Sink-only entries are read-only,
and selection does not start hardware or grant Route authorization.

ADR 1400 selects one Vue/Tauri presentation with platform-owned Node hosts.
`ANDROID-A01` records the ordered migration from the current audio app plus
camera/controller/touchpad lab applications to one Android Node. No Android
permission or foreground-service declaration was changed in this slice.

The follow-on A01B1 slice adds a bounded schema-v1 Android Capability module
registry to the existing service-owned Node identity. The live audio module is
attached; camera, IMU, gamepad, haptics and touchpad are present only as
explicitly Offline/unattached migration entries. The service Binder exposes the
catalog without moving lifecycle ownership into the Activity.

A01B2 adds read-only local hardware presence to that catalog. The service probes
microphone/audio-output, camera, accelerometer+gyroscope, touchscreen and
vibrator inventory without opening hardware. Presence, Adapter attachment and
availability remain three separate states, so detected-but-unintegrated input
hardware is still Offline.

A01B3 now attaches the first input lifecycle facade. The unified Node service
owns accelerometer/gyroscope registration through an IMU Adapter and exposes
generation-bound start/stop plus observer state through its Binder. IMU is
therefore an attached Available/Busy/Offline Source in the local catalog; at
that slice, gamepad, haptics, touchpad and camera remained explicitly
unattached. The sensor
callback retains only fixed-size latest values and does not perform network or
UI work. Sample data-plane routing and background operation are not claimed.

The following A01B3 slice attaches the touch-gamepad state boundary. The Node
service now owns its generation and complete bounded control snapshot; Binder
clients may start/stop it and submit only whole frames tagged with the current
generation. Stop immediately restores a neutral frame, and delayed frames from
an earlier session are rejected. The standalone Controller Lab and its UDP
sender are unchanged. Haptics remains Offline/unattached: Android Lint confirmed
that a real `Vibrator` Sink needs `android.permission.VIBRATE`, and this
integration scope does not authorize a Manifest permission change.

A01C1 now makes the Android build consume the same Vue/Vite presentation source
as the Windows Tauri host. Gradle builds and stages that output under
`assets/capyio-ui`; a verification task requires the entrypoint, JavaScript and
CSS and rejects root-absolute asset paths. The assembled debug APK contains the
three asset classes. The existing native Activity remains the runtime UI, so
Android visual parity and the Binder-to-shared-DTO observer bridge are still
A01C2 work.

A01C2 adds a transitional, actually loadable Android observer surface while
preserving the native audio controls as a fallback. The packaged Vue workspace
reads the service-owned seven-Capability catalog through one no-argument bridge
method and labels the backend `android_observer`; it exposes no mutable command,
fake peer, Route or simulated panel. WebView network/content access and external
navigation are blocked, and Back returns to the proven native controls. Physical
rendering and mutable-operation parity are not claimed.

That label describes the original A01C2 read-only surface. A01D6 changes the
current schema-4 discriminator to `android_node` now that bounded lifecycle,
input, audio and publication-selection mutations are attached; it does not
imply Route or peer-catalog support.

A01D1 connects the first shared UI operations: schema-v2 IMU and touch-gamepad
Quick Actions now start, retry or stop their existing Service-owned Adapter
lifecycles. They are explicitly `capability_control`, keep every Route field
null and do not select sharing or transmit data. The native bridge accepts only
the two fixed action IDs and operations valid for the observed lifecycle state;
unknown, malformed and invalid-transition requests fail closed. Audio, camera,
haptics, touch surface input and cross-node Routes remain outside this slice.

A01D2 connects the shared Controller surface to the Service-owned touch-gamepad
state. The bridge admits only bounded button, d-pad, stick, trigger and reset
commands; each is merged into and synchronously submits a complete snapshot with
the active generation. Stop neutralization and stale-generation rejection remain
inside the Service contract. Android hides the desktop receiver/projection
panels and marks unavailable motion honestly. This does not create sharing,
network transport or a Route, and it has compile/package rather than device UI
evidence.

A01D3 attaches the shared Touchpad surface to its own Service lifecycle. The Vue
component captures at most five stable Pointer contacts, normalizes and
animation-frame-coalesces complete state, and releases contacts on cancel, focus
loss, page exit and Stop. The local bridge accepts only a bounded canonical
integer grammar; Java revalidates every contact and the Service owns sequence and
generation. The separate JNI packet Source and Windows Precision Touchpad Sink
remain unconnected, so this slice claims local touch input only.

A01D4 moves the existing Android microphone and speaker operations into shared
Quick Actions and adds a common audio status panel. The DTO exposes independent
lifecycle generation, actual negotiated format, processed frames, permission
readiness and bounded problem codes. Start returns to the Activity main thread
and therefore preserves Android's system permission UI; Stop uses the existing
Service actions. No permission or foreground-service declaration changed, and
the native controls remain available until shared-UI physical parity is proven.

A01D5 enables the common local-hardware sharing checkboxes for Android. The
Service admits only exact current-catalog IDs whose hardware is present, Adapter
is attached and Ports include a Source. Microphone, IMU, gamepad and touchpad
therefore qualify; speaker, haptics and the unattached camera do not. The state
is session-local publication intent and neither starts hardware nor establishes
peer discovery, authorization, a Route or a data plane.

A01E composes the existing headless Camera2/MediaCodec pipeline into the same
`dev.capyio.android` APK and Node catalog. A narrow facade fixes the current
default to the back camera, balanced AVC and existing ADB-reverse development
endpoint; the unexported camera foreground service still owns capture, encoding,
export and cleanup. The common UI adds a lifecycle-valid camera Quick Action and
camera Source publication selection, while permission, capture, selection and
Route state remain separate. The standalone camera lab is retained until source,
quality and end-to-end Route controls reach parity.

The physical WebView run also exposed that the earlier `file:` host did not load
the packaged ES module or stylesheet because its origin was null. The unified
host now serves only the packaged subtree from a fixed APK-only HTTPS origin,
with file/content access disabled and every non-CapyIO request denied.

A01H attaches the remaining Android haptics catalog entry. The Node Service owns
one generation-bound vibrator Sink, the shared UI controls its lifecycle, and a
bounded Binder command preserves normalized strength and maximum-duration
validation while projecting two rumble channels onto the phone's single
actuator. The explicitly approved `VIBRATE` permission has no runtime prompt.
This provides the local Sink boundary; Windows feedback delivery still needs an
authorized Route.

A01F1 hardens the already unified Controller and Touchpad lab Routes. Service
lifecycle mutation and UI frame submission now share one serialization
boundary. The current generation is committed before Adapter enqueue, so a
stale WebView event cannot enter either data plane. Bounded queue rejection is
no longer an ambiguous UI-only failure: the affected Capability enters Failed,
Controller pending frames are discarded back to neutral, and Touchpad stops
through its existing cancel/close path. Hardware-free ordering tests cover
stale rejection, commit-before-delivery and explicit delivery rejection.

A01F2 adds the corresponding UI foreground-loss boundary. Activity pause first
closes admission for later JavaScript events, then asks the Service to submit a
neutral gamepad state and empty touchpad contacts only when the current Active
state needs release. It does not stop either Capability or any unrelated audio,
camera or IMU lifecycle. Pure policy coverage verifies stopped-state rejection,
active release and idempotence for already-neutral controls.

A01F3 corrects the service-ownership defect found during the first A01F2 device
probe. A gamepad-only session safely released its pressed state on Home, but the
bound-only Node service was then destroyed. IMU, gamepad, touchpad and haptics
now use closed started-service actions and retain the Android
`connectedDevice` foreground type while their Adapter lifecycle owns work.
`FOREGROUND_SERVICE_CONNECTED_DEVICE` plus the normal `CHANGE_NETWORK_STATE`
prerequisite are declared under ADR 1421. The new APK passes the Android gate;
installation and physical background/reopen/Stop evidence remain pending exact
approval for the current wireless-debug endpoint.

A01E2 closes the unified camera source/quality UI gap. The common Vue panel now
refreshes the existing bounded Camera2 inventory after consent and selects an
exact source/vendor-Zoom token plus one of three AVC presets only while stopped.
The V2419A enumerated seven choices; selecting front `1@auto` and `clear` caused
Camera Service to open ID 1. Active mutation was rejected and Stop released it.

## Automated evidence

- 2026-09-17 A01F1/A01F2: `platform/android/gradlew.bat capyioCheck --offline`
  passed compilation, Lint, shared-UI/JNI/APK composition and 255 Android
  composition assertions; `cargo xtask ci` also passed the complete repository
  matrix. No APK or driver was installed.
- 2026-09-17 A01F3: the Android gate passed with 262 composition assertions,
  warnings-as-errors Lint and integrated debug APK packaging. The attempted
  install on the new endpoint was blocked before execution, so no
  permission-changing APK was installed and no physical result is claimed.
- `cargo test --offline -p capyio-input -p capyio-dsu-adapter -p capyio-viiper-adapter -p capyio-windows-input -p capyio-desktop`: passed after the gamepad merge.
- `cargo test --offline -p capyio-testkit -p capyio-desktop`: passed after schema 4 (15 testkit tests; 54 desktop tests passed and 6 physical tests remained ignored).
- `npm run build` in `apps/desktop`: TypeScript and Vite production build passed.
- local browser rendering: 7 local and 7 peer Capability cards, exactly 2
  selectable local Sources, no horizontal overflow at 1265 px or 375 px content
  width, one-column 297 px mobile cards, successful checked-state refresh and
  no browser warning/error logs.
- `cargo xtask validate-docs`: passed after schema 4 (88 unique Requirement IDs).
- `cargo xtask validate-manifests`: passed.
- `cargo xtask check`: final workspace check and Clippy with warnings denied passed after schema 4.
- `cargo xtask ci`: passed end to end, including the workspace tests, fixture
  demo, repository validation, Adapter smoke test, frontend typecheck and
  production build.
- `platform/android/gradlew.bat :node-contract:check --offline`: passed with 16
  Capability-catalog, 36 audio-lifecycle and 126 native-LAN assertions.
- `platform/android/gradlew.bat :app:lintDebug :app:assembleDebug
  :app:verifyDebugEntrypoints --offline`: passed; the debug APK was built only
  as evidence and was not installed.
- `platform/android/gradlew.bat :app:capabilityCompositionTest --offline`:
  passed 39 hardware-free assertions over the exact service composition.
- `platform/android/gradlew.bat :node-contract:capabilityLifecycleContractTest
  --offline`: passed 40 generation-bound lifecycle and gamepad-state assertions.
- `platform/android/gradlew.bat capyioCheck --offline`: final aggregate Android
  Gate passed all contracts, composition checks, Lint, entrypoint verification
  shared-Vue asset verification and debug assembly without installing the APK.
- the Android composition executable passed 170 assertions after A01D5, including
  schema-4 projection, all seven capabilities, Source-only sharing eligibility,
  selection identity/bounds/idempotence, JSON escaping
  and disconnected-service output plus closed IMU/gamepad Quick Actions and
  command rejection, complete touch-gamepad state composition and bounded
  Android gamepad-state projection plus touchpad attachment, complete-frame
  bounds, lifecycle neutralization and bridge parsing plus independent audio
  action admission, permission projection and actual-format/frame state.
- the A01E composition executable passed 182 assertions, adding attached camera
  catalog/format/permission/lifecycle/publication checks and exact camera command
  admission/rejection.
- the A01H composition executable passed 193 assertions, adding attached Sink
  availability, haptics lifecycle admission, generation/strength/duration bounds
  and malformed-command rejection.
- an authorized V2419A Android 16 run installed the unified debug APK and rendered
  the shared Vue UI. Its camera action produced an active unexported foreground
  service and Camera Service client for `dev.capyio.android` on ID 0; UI Stop
  removed the service, recorded DISCONNECT and left no active camera client. The
  final installed `0.5.0-dev` APK SHA-256 is
  `07C6D2943C5ED98D824838FDD7DD61145A81DC6759B7F98B8EED74BCAF04EC39`.
- the haptics `0.5.1-dev` APK SHA-256 is
  `EAAA1FAB46268FA2CAEF3A1DC6053DD2EA6A963F95FF3BBCC1F055FFE60BAAEF`.
  The VIBRATE permission was granted; shared-UI Start reached Active and Android
  recorded CapyIO's bounded 35 ms / 0.28 request. The OS suppressed it under the
  current haptic setting, so tactile perception is not claimed. UI Stop returned
  Idle and `CurrentVibration` was null.
- the installed `0.5.2-dev` APK SHA-256 is
  `E55B6305E1F1DC143C50F134A0AE9A446F6DD8795C708B6EE0768760F7E79223`;
  shared-UI Camera2 inventory/selection physically opened and released front ID 1.
- the installed unified `0.5.4-dev` APK SHA-256 is
  `E1335AAD51CBE4DCCA76516326633A83AA0E3C4DAECD467AD5645D58E01AC2F4`.
  One-device automation passed camera AVC/NV12 decode, touch-gamepad plus IMU,
  speaker playback and phone-to-Windows microphone network arrival. Touchpad
  reached the Windows synthetic submission boundary but returned Win32 error 5;
  the common desktop UI now exposes the stable permission-denied problem rather
  than a generic helper exit. No VHF or driver operation was performed.
- `jar tf platform/android/app/build/outputs/apk/debug/app-debug.apk`: confirmed
  `assets/capyio-ui/index.html` and generated JavaScript/CSS in the assembled
  archive; the entrypoint references both assets with `./assets/...` paths.
- the installed unified `0.5.5-dev` APK (version code 17, SHA-256
  `728992717F6615EACE082B2FFB40D2D147846C45D6D18A90E1470C0635284FED`) cold
  launched directly into shared Quick Actions with correct system-bar insets.
  Shared UI opened the retained native diagnostics page and returned to the
  same unified view without exposing it as the default product screen.
- the already installed VHF interface was selected by read-only probe. The
  elevated, hash-pinned desktop-owned touchpad receiver processed and submitted
  seven frames, observed `0->1->2->0` contacts and closed normally. No driver
  mutation occurred. Desktop tests cover installed/synthetic selection and the
  stable elevation-required problem code.
- the 0.5.5 microphone action and Windows service reached active, but the
  repeat ordinary WASAPI client failed endpoint initialization with
  `0x80070057` after successful enumeration. Earlier non-zero capture evidence
  is not invalidated, but this host regression remains open and this run does
  not claim a fresh capture-ring pass.

## Unified touchpad parity follow-up (2026-09-19)

The authenticated touchpad Route now preempts the obsolete loopback worker,
uses bounded/coalescing Android delivery under normal input pressure, and uses
the production VHF ClickPad sink with the standalone lab's double-tap-drag
latch. The Android shared UI switches an active touchpad Route into a dedicated
full-screen surface with no navigation controls, keeps the display awake,
excludes system-edge gestures, and emits distinct tap/drag haptic feedback.
Cancellation and multi-contact input do not produce false tap vibration.

The installed Lenovo Y700 debug APK SHA-256 is
`03758C2508B5F62E2A2C2C852CD528F3477D531D05ADE5B7DF5C4FD5317A70FB`.
Its shared UI rendered after cold start and retained the paired Node identity.
Android composition passed 283 assertions; the remote-touchpad latch tests and
83 desktop unit tests passed. A direct, separately authorized VHF fixture was
acknowledged by the installed driver but did not move the current Windows
cursor. PnP inspection reports the CapyIO parent, VHF lower filter, and HID
Precision Touchpad child all started with problem code zero. The current
interactive session is `rdp-tcp#0`, whose visible pointer uses the separate
Remote Desktop input device rather than the host Precision Touchpad stack. The
desktop preflight now fails explicitly in that topology instead of claiming a
usable VHF projection. A subsequent GameViewer-backed `console` session reran
the exact hash-pinned four-frame acceptance: all frames were acknowledged, but
the cursor remained `(720,450)` with delta `(0,0)`. `SPI_GETTOUCHPADPARAMETERS`
reported the device present, enabled and active with five contacts, tap,
tap-and-drag, pan and zoom enabled; PnP still reported no problem. RDP was
therefore a separate invalid observation surface, not the sole failure cause.
With explicit approval, only the exact existing
`ROOT\SYSTEM\0001` CapyIO VHF device was restarted; no package was installed,
removed or replaced. The fixed four-frame acceptance then moved the cursor from
`(725,455)` to `(1092,667)`, confirming that the prior zero-delta result was a
stale VHF device state rather than an Android frame-format defect.

The product Route was then exercised end to end rather than through the fixture.
The hash-pinned desktop CLI exposed its exact pending Route ID, the lab script
approved only that Route through the Android bridge, and the authenticated mTLS
touchpad stream switched the shared Android UI to its dedicated immersive
surface. The final bounded run approved Route
`<LAB_ID_11>`, advanced the Android touch sequence to
20, and moved the Windows cursor from `(1106,270)` to `(1153,270)`. The CLI
emitted `CAPYIO_TOUCHPAD_DATA_PASSED` and the outer acceptance exited zero.
After disconnect, Android reported `idle`, no contacts, no pending consent and
no immersive surface, providing explicit stuck-contact cleanup evidence. This
automation proves one-contact pointer delivery; physical two-to-five-finger
Windows gestures and perceived haptic feedback still require a manual human
pass.

## Touchpad high-rate regression follow-up (2026-09-19)

The first human GameViewer trial could move the pointer and scroll with two
fingers, but both felt laggy; the tablet then returned from the immersive
surface to the main UI. Android remained alive, so this was a touchpad
Capability/Route exit, not an OS process crash. A later read showed
`CAPY.ANDROID.TOUCHPAD_RECEIVER_UNAVAILABLE`, which is the local-lab retry
failure and does not establish the original secure Route's failure cause.

Source inspection found a concrete high-rate UI amplification: every accepted
touch frame dispatched a WebView native-state event and caused the shared UI
to reread the full Node snapshot plus other hardware panels. Per-frame full-
state dispatch was removed; lifecycle changes still notify the UI. While the
immersive surface is active, the 500 ms poll reads only touchpad lifecycle.
The hidden second touchpad component is also unmounted during immersive input.

The replacement unified debug APK SHA-256 is
`0A3FEA0DCACAE397540BDB303E35209DB09C82F6EA3ECE5887763FF46CE56EFD`.
On the same Y700, the final eight-swipe alternating-direction Gate entered the
actual immersive surface and moved the Windows pointer on every swipe (per-
swipe Manhattan travel 424–542 px, total 3747 px). The authenticated mTLS
Route remained active for the bounded 15-second Gate, exited cleanly, and the
outer acceptance returned zero. Android `capyioCheck` passed, including 283
composition assertions and Lint. This is burst-stability evidence, not a
claim that multi-minute human use or native two-finger scroll is now smooth;
those still need repeat manual verification.

## Explicit exclusions

- no push, pull request, package publication or production signing;
- no Android permission/service change, production signing or other-device APK
  installation; only explicitly authorized unified debug APK updates were
  installed on the identified lab devices;
- no Windows driver install/removal/replacement or boot/security-policy change;
  the exact existing CapyIO VHF device was restarted once with explicit approval;
- no Display merge or Display worktree modification;
- no claim that the Tauri schema-4 fixture catalog is real hardware discovery;
- no claim that Android publication selection has reached a remote peer;
- physical Android rendering, camera decode, gamepad/IMU, speaker and installed-
  VHF touchpad paths are proven. Microphone arrival and an earlier ordinary-
  client capture are proven, but the current host's repeat WASAPI initialization
  regression remains open. No claim is made for complete Route/peer parity or
  camera denial/revoke/background qualification.
