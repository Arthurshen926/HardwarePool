# Non-display integration inventory

Date: 2026-09-04

Integration branch: `codex/capyio-integrated-alpha`

## Scope

This inventory records source-level integration of the completed non-display
vertical prototypes. It distinguishes merged code from product composition and
physical deployment. Display is deliberately excluded and its branch/worktree
is not modified.

| Capability | Integrated source baseline | Android boundary | Windows boundary | Current product-composition status |
|---|---|---|---|---|
| Bidirectional audio | `codex/capyio-native-audio-subsystem` at `50f3921` | unified audio Node service plus shared status/Quick Actions for independent microphone Source and speaker Sink | unified desktop Quick Actions plus service/host and virtual audio boundaries | shared operation surface and physically proven native paths are present; shared-UI device parity and production pairing/security remain open |
| Camera | `codex/capyio-camera` at `9808d2c` | unified APK composes the service-owned Camera2/MediaCodec lifecycle, concrete source/vendor-Zoom selection, AVC quality and private transport behind a narrow facade; standalone lab retained | Media Foundation virtual-camera, shared-memory ingress and host crates | shared UI and catalog attach/configure the local Source; product discovery/pairing/Route composition and endpoint parity remain open |
| Precision Touchpad | `codex/capyio-touchpad` at `77b6846` | unified Node owns a generation-bound five-contact shared-UI state facade; separate JNI packet Source/lab remains intact | user-mode/VHF projection boundaries and Runtime workers | local lifecycle/UI state is attached; authenticated product Route and Windows delivery remain open |
| Touch + motion gamepad | `codex/capyio-gamepad` at `cadaef9` | unified Node owns IMU, generation-bound gamepad state and a bounded service-owned vibrator Sink; standalone touch Controller Lab remains intact | shared Controller UI, DSU/VIIPER/DS4 and optional XInput projection boundaries | local lifecycle/state and haptics command projection are attached; authenticated gamepad/feedback Routes remain open |
| IMU | audio baseline plus gamepad composition | unified Service-owned SensorManager Adapter with generation-bound start/stop; SensorServer lab remains separate | live numeric panel, recorder fixture and DSU/DS4 consumers | lifecycle attached; typed data-plane routing, transport/security and shared UI selection remain open |

Portable Video/Input contracts from `codex/capyio-contracts` at `fc3da363`
were integrated before the three hardware feature branches.

## Local integration commits

| Commit | Result |
|---|---|
| `8cfc16b` | portable video and input contracts integrated |
| `684ead1` | camera-sharing prototype integrated |
| `d2fc1ff` | Precision Touchpad prototype integrated |
| `7798993` | touch + motion gamepad prototype integrated |

The branch already started from the complete native bidirectional-audio
baseline. These commits are local only and are not pushed.

## Unified UI state

`UiSnapshot` schema 4 and the shared Vue `CapyIOApi` now expose:

- local and peer Capability catalogs from the Runtime snapshot;
- availability and permission requirement;
- each Port's Source, Sink or Control direction, Profile and format;
- per-local-Source publication selection with a narrow allow-listed command;
- explicit fixture labels so catalog projection is not mistaken for physical
  hardware enumeration or authorization.

The UI intentionally uses “本机能力” and “远端节点公开的能力” rather than
global sender/receiver roles. One Node can simultaneously expose Source, Sink
and Control Ports.

The Android service-owned schema-v1 catalog now also records read-only hardware
presence for microphone, audio output, camera, accelerometer+gyroscope,
touchscreen and vibrator. Presence is not treated as Adapter attachment or
sharing availability. A01C1 makes the Android build package the exact shared
Vue/Vite output under `assets/capyio-ui`, with relative-path and entrypoint asset
checks. A01C2 adds a local-only, read-only Android observer that renders this
packaged workspace on demand and projects the service catalog into shared schema
4. The native audio controls remain available as the fallback, and Back returns
to them. The observer exposes no sharing selection, Route or hardware lifecycle
command and does not invent peer state; those operations remain separate work.

A01D1 adds shared schema-v2 `capability_control` actions for the already attached
IMU and gamepad lifecycles. Their fixed start/retry/stop commands cross the local
bridge into the Node Service, while sharing selection and all Route fields remain
separate and unset. No audio permission flow, touch-state submission or remote
transport is exposed by this slice.

A01D2 connects the shared Controller surface to that Service-owned gamepad
facade. Android exposes only bounded local button/d-pad/stick/trigger/reset
commands; Java merges them into complete generation-scoped snapshots and Stop
neutralizes the state. Windows/DSU receiver controls are hidden on Android,
motion is marked unavailable rather than fabricated, and no sharing or Route is
created.

A01D3 attaches the shared Touchpad surface to a separate Service-owned lifecycle.
It accepts complete normalized zero-to-five-contact frames through a closed local
bridge, advances sequence only in the active generation, coalesces MOVE updates
and releases all contacts on cancellation, focus loss, page exit or Stop. The
existing JNI/Windows packet path remains a separate data-plane integration task.

A01D4 adds Android microphone and speaker Quick Actions plus a shared audio state
panel. It reuses the existing Activity-owned recording/notification permission
flow and Service commands, exposes actual negotiated format and frame counts,
keeps both directions independent, changes no Manifest declaration, and retains
the native audio controls until shared-UI device parity is proven.

A01D5 enables the shared local-capability checkboxes against the Service-owned
Android catalog. The automatically probed, attached microphone, IMU, gamepad,
touchpad and, after A01E, camera Source capabilities may record session-local
publication intent; speaker/haptics Sinks cannot. This does not start an
Adapter, grant permission, create a Route or exchange a catalog with a peer.

A01E composes the existing unexported camera foreground service into the unified
APK, maps its lifecycle into the local catalog and adds a closed camera Quick
Action. The shared WebView now serves the common Vue assets from a fixed
APK-only HTTPS origin so ES modules and styles render on a physical device while
all non-CapyIO requests fail closed. An authorized V2419A run proved UI-driven
Camera2 connect/disconnect and complete service/resource cleanup.

## Remaining integration work

The control protocol now decodes a complete CatalogSnapshot into a validated
Core value with mandatory Session identity, and the Runtime has a
session-bound whole-peer-catalog admission method. It rejects the wrong Node
identity or a non-Ready Session without mutation, preserves locally observed
online state and atomically reconciles dependent Routes. A shared control-host
crate now composes the decoder and Runtime admission for one caller-asserted
Session/peer tuple with bounded recent-message deduplication. No platform
transport constructs that binding yet, so this is product-composition
infrastructure rather than a discovery or pairing claim.

The Windows Tauri host no longer retains `DemoLab` as its long-lived state.
It consumes the fixture once at startup, then all desktop feature workers and
UI commands share a process-owned `NodeRuntime`; only the bootstrap Session and
demo Route allow-list remain alongside it. This establishes the state-ownership
seam needed for a future authenticated control connection without claiming that
such a connection exists today.

1. Route touchpad, IMU, gamepad and haptics through bounded typed data planes.
   Their local lifecycles/UI state and publication selection are attached, but
   peer catalog exchange and Route authorization are not. The Android vibrator
   Sink is attached, but Windows rumble feedback is not yet routed to it.
2. Physically verify the shared audio controls and remove the transitional
   native fallback only after equivalent permission, lifecycle and device evidence.
3. Implement real Adapter-driven local hardware discovery and an authenticated
   platform transport that constructs the new control-host binding and updates
   the now process-owned desktop Runtime; schema 4 currently projects
   deterministic bootstrap catalogs.
4. Connect touchpad and camera to authenticated product Routes rather than
   local lifecycle and lab transport boundaries; add authenticated endpoint
   parity before retiring the standalone lab.
5. Keep Display out until its branch reaches its own completion gate.
