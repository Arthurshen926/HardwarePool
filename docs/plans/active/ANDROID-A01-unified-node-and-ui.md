# ANDROID-A01 — Unified Android Node host and shared UI

Status: active

Owner: Codex

Created: 2026-09-04

Requirements: `FR-NODE-002..004`, `FR-CAP-001..006`, `FR-UX-002..006`,
`FR-PLAT-002..006`, `NFR-SEC-002..005`, `NFR-STAB-001..004`

## Objective

Ship one Android CapyIO application and one Node identity for microphone,
speaker, camera, IMU, touch gamepad and touchpad capabilities, using the same
Vue/TypeScript presentation as Windows while retaining native platform-owned
permissions, foreground services, hardware callbacks and bounded data workers.
Display is excluded until its feature branch is complete.

## Current baseline

- `platform/android/app` is the proven microphone/speaker Node host and service.
- `platform/android/app` now composes the proven camera service sources behind a
  narrow facade; the standalone camera lab remains as a rollback/parity harness.
- `apps/android/controller-lab` is a separate touch+IMU controller lab.
- `platform/android/touchpad-bridge/lab-app` is a separate touchpad lab.
- `apps/desktop/src` is the schema-4 shared presentation source. A01C1 packages
  its exact Vite output into the Android APK; A01C2 renders it on demand through
  a read-only catalog bridge while retaining native audio controls as fallback.

## Ordered slices

1. **A01A — catalog/UI contract (complete):** project local and peer Capability
   catalogs through `UiSnapshot` schema 4; show Source/Sink/Control directions;
   allow only local Source publication selection; keep fixture state labeled.
2. **A01B — native module registry (IMU and gamepad state attached):** A01B1 adds the
   bounded schema-v1 Android-local DTO and service-owned registry, attaches the
   existing live audio façade and lists camera, controller and touchpad modules
   as explicitly Offline/unattached. A01B2 adds permission-free, read-only
   hardware presence probes while keeping presence, attachment and availability
   separate. A01B3 supplies the generation-bound non-audio lifecycle contract,
   attaches accelerometer/gyroscope registration to the unified Service, and
   attaches a bounded, generation-scoped touch-gamepad snapshot facade that
   neutralizes on Stop. Its shared UI surface and touchpad were completed by
   A01D; the service-owned haptics Sink and its bounded feedback command are
   completed by A01H after explicit `VIBRATE` approval. Standalone labs remain
   available as parity harnesses.
3. **A01C — Android observer shell (complete):** A01C1 builds the
   shared Vue source and packages it under `assets/capyio-ui`, with relative-path
   and required-asset gates. A01C2 renders it on demand in a network-blocked
   local host and projects the Service catalog through one no-argument read-only
   method. Back restores the native control fallback. The existing Node service,
   not the Activity or WebView, remains lifecycle owner.
4. **A01D — per-capability operations (IMU/gamepad lifecycle attached):** A01D1
   exposes schema-v2 `capability_control` Quick Actions for the existing IMU and
   gamepad Service lifecycles through an exact state-dependent command allow-list.
   A01D2 connects the shared Controller to the complete generation-bound local
   gamepad state and hides desktop-only receiver/projection controls on Android.
   A01D3 attaches a separate generation-bound five-contact Touchpad lifecycle
   and shared surface, retaining the standalone JNI/transport lab for later Route
   composition.
   A01D4 adds independent microphone/speaker actions and an actual-format/frame
   status panel while retaining Android-owned permission prompts and the native
   fallback.
   A01D5 adds Service-owned session-local publication selection for the four
   attached, hardware-present Source capabilities. Sink/unattached/foreign
   entries are rejected. Selection remains separate from permission, Adapter
   lifecycle, Routes and peer catalog exchange.
   A01D6 renames the shared schema discriminator from the obsolete read-only
   `android_observer` label to `android_node` without widening its authority.
   These operations do not create Routes. Audio permission UX, Route status and
   the remaining capabilities proceed in separate slices.
5. **A01E — camera integration (positive path complete):** after explicit
   approval, the unified APK adds optional camera inventory plus CAMERA/camera
   foreground-service declarations, attaches the existing service-owned
   Camera2/MediaCodec Source and exposes a closed shared-UI action. An authorized
   V2419A run proves system permission, active Camera2 ownership and UI-driven
   release. A01E2 adds bounded concrete Camera2 source/vendor-Zoom and AVC
   quality selection to the shared UI and physically proves front-camera ID 1.
   Denial, revoke, process-death and long-background qualification remain.
6. **A01F — input data-plane integration (bounded lab Routes attached):** the
   unified Service now owns the Controller/IMU UDP publisher and private
   touchpad record sender used by the Windows receivers. A01F1 serializes
   lifecycle mutation with UI submission, commits the observed generation
   before Adapter delivery, rejects stale frames before enqueue and turns queue
   pressure into an explicit Capability failure plus neutral/cancel cleanup.
   A01F2 rejects new UI input after Activity pause and synchronously submits a
   neutral gamepad snapshot plus empty touchpad contacts without stopping the
   Service or unrelated capabilities. Authenticated product transport and
   physical foreground-loss qualification remain.
7. **A01G — migration closeout:** remove redundant product launchers only after
   parity evidence, rollback instructions and the unified APK gate pass.
8. **A01H — haptics Sink (complete):** after explicit approval, declare the
   normal `VIBRATE` permission, attach a generation-bound single-actuator
   projection of `dual-rumble-v1`, expose shared-UI lifecycle control and a
   bounded Binder command. Start emits one short visible-action confirmation
   pulse; remote feedback still requires an authorized Route.

## Acceptance

- one Android application owns one stable Node identity and capability catalog;
- Windows and Android render the same shared UI source and DTO major;
- local sharing selection, permission, Route authorization and Route lifecycle
  remain distinct observable states;
- no UI/window lifecycle owns the Node service;
- microphone, camera, sensor and touch callbacks retain their non-blocking
  bounded-worker rules;
- standalone labs remain available until their replacement passes equivalent
  hardware evidence;
- no Display code enters this plan.

## Approval boundaries

The A01E CAMERA/foreground-service declarations and one designated debug APK
installation were separately authorized on 2026-09-16. This plan does not
authorize any other permission, driver operation, device reset, production
signing or remote publication.
