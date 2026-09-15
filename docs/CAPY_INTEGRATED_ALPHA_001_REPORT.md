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
Tauri host to `UiSnapshot` schema 4. The same Vue/TypeScript component displays
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

## Automated evidence

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
- `jar tf platform/android/app/build/outputs/apk/debug/app-debug.apk`: confirmed
  `assets/capyio-ui/index.html` and generated JavaScript/CSS in the assembled
  archive; the entrypoint references both assets with `./assets/...` paths.

## Explicit exclusions

- no push, pull request, package publication or production signing;
- no APK installation and no Android permission/foreground-service change;
- no Windows driver install/removal, restart or boot/security-policy change;
- no Display merge or Display worktree modification;
- no claim that the Tauri schema-4 fixture catalog is real hardware discovery;
- no claim that Android publication selection has reached a remote peer;
- no claim of physical Android rendering or complete mutable-command UI parity;
  A01D4 adds shared audio operations while retaining native controls as fallback.
