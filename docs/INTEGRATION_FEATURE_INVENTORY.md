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
| Bidirectional audio | `codex/capyio-native-audio-subsystem` at `50f3921` | unified audio Node service with independent microphone Source and speaker Sink | unified desktop Quick Actions plus service/host and virtual audio boundaries | physically proven vertical paths are present; production pairing/security remain open |
| Camera | `codex/capyio-camera` at `9808d2c` | separate Camera2/MediaCodec lab and private AVC record/transport boundary; main Node catalog now declares an unattached Offline camera module | Media Foundation virtual-camera, shared-memory ingress and host crates | source and migration inventory are merged; camera lifecycle is not yet attached to the Android product app |
| Precision Touchpad | `codex/capyio-touchpad` at `77b6846` | separate touchpad lab/JNI bridge and bounded private packet Source; main Node catalog declares it Offline/unattached | user-mode/VHF projection boundaries and Runtime workers | source is merged and catalogued; product Route/UI operation remains to be composed |
| Touch + motion gamepad | `codex/capyio-gamepad` at `cadaef9` | unified Node owns IMU sensors plus a generation-bound, neutralizing gamepad state facade; standalone touch Controller Lab remains intact; haptics is Offline/unattached pending Manifest permission approval | shared Controller UI, DSU/VIIPER/DS4 and optional XInput projection boundaries | IMU and gamepad lifecycle/state are attached; shared touch surface, feedback and product Routes remain open |
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

## Remaining integration work

1. Connect the shared touch-gamepad surface to the Service-owned state facade,
   attach touchpad lifecycle without regressing standalone evidence, and route
   IMU/gamepad samples through bounded typed data planes. Lifecycle Quick Actions
   are attached, but they are not sharing or Route authorization. Haptics requires
   separate approval for the Android `VIBRATE` Manifest permission.
2. Replace the transitional native-control fallback only after A01D maps
   schema-versioned shared UI operations to the Service with equivalent
   permission, lifecycle and physical-device evidence.
3. Implement real Adapter-driven local hardware discovery and authenticated
   peer catalog exchange; schema 4 currently projects deterministic Runtime
   fixture catalogs.
4. After explicit permission approval, attach camera lifecycle; connect
   touchpad and camera to product Route actions rather than lab-only
   entrypoints.
5. Keep Display out until its branch reaches its own completion gate.
