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
- `platform/android/capyio-camera-app` is a separate camera lab application.
- `apps/android/controller-lab` is a separate touch+IMU controller lab.
- `platform/android/touchpad-bridge/lab-app` is a separate touchpad lab.
- `apps/desktop/src` is the schema-4 shared presentation source, but no Android
  Tauri host consumes it yet.

## Ordered slices

1. **A01A — catalog/UI contract (complete):** project local and peer Capability
   catalogs through `UiSnapshot` schema 4; show Source/Sink/Control directions;
   allow only local Source publication selection; keep fixture state labeled.
2. **A01B — native module registry (IMU and gamepad state attached):** A01B1 adds the
   bounded schema-v1 Android-local DTO and service-owned registry, attaches the
   existing live audio façade and lists camera, controller and touchpad modules
   as explicitly Offline/unattached. A01B2 adds permission-free, read-only
   hardware presence probes while keeping presence, attachment and availability
   separate. A01B3 now supplies the generation-bound non-audio lifecycle
   contract and attaches accelerometer/gyroscope registration to the unified
   Service. The next slice attaches a bounded, generation-scoped touch-gamepad
   snapshot facade that neutralizes on Stop; its shared UI surface, haptics and
   touchpad remain pending while standalone labs are preserved. Haptics is held
   behind separate approval for the `VIBRATE` Manifest permission. None of these
   slices adds a permission or service declaration.
3. **A01C — Android Tauri observer shell:** build the shared Vue source inside
   the Android package and implement the same read-only catalog commands through
   a Kotlin plugin/host boundary. The existing Node service, not the Activity,
   remains lifecycle owner.
4. **A01D — per-capability operations:** connect schema-versioned start/stop,
   permission and Route status commands one capability at a time. Audio goes
   first because its independent service lifecycle is already physically
   proven.
5. **A01E — camera integration:** only after explicit approval, add the camera
   permission/service declarations required by the selected lifecycle and run
   denial/revoke/background tests on an authorized device.
6. **A01F — controller/touchpad integration:** replace the standalone launchers
   with built-in shared UI surfaces and route their bounded native sources into
   the Node-owned sessions. Retain exact foreground-loss and neutral/cancel
   behavior.
7. **A01G — migration closeout:** remove redundant product launchers only after
   parity evidence, rollback instructions and the unified APK gate pass.

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

This plan does not itself authorize an Android manifest permission change,
foreground-service declaration change, APK installation, driver operation,
device reset, production signing or remote publication.
