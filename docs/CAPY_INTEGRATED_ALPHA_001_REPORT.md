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
  passed 19 hardware-free assertions over the exact service composition.
- `platform/android/gradlew.bat capyioCheck --offline`: final aggregate Android
  Gate passed all contracts, composition checks, Lint, entrypoint verification
  and debug assembly without installing the APK.

## Explicit exclusions

- no push, pull request, package publication or production signing;
- no APK installation and no Android permission/foreground-service change;
- no Windows driver install/removal, restart or boot/security-policy change;
- no Display merge or Display worktree modification;
- no claim that schema-4 fixture catalogs are real hardware discovery;
- no claim that the Android product UI is already unified.
