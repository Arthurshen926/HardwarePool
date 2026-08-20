# HardwarePool Toolchain

> Versions are the bootstrap baseline as of August 2026. Update them through an explicit dependency/toolchain change, not ad hoc on one workstation.

## Shared development

- Git 2.4x or newer;
- Rust 1.97.1 via rustup;
- `rustfmt` and Clippy components;
- Node.js 24 LTS;
- Corepack and pnpm 11.5.3;
- Python 3.11+ for test analysis scripts;
- Protobuf compiler is supplied to Rust builds by `protoc-bin-vendored`.

The frontend currently declares Tauri CLI 2.11.2, Tauri JavaScript API 2.11.1,
Vue 3.5.x and Vite 8.x. The JavaScript API uses 2.11.1 because 2.11.2 was
never published to npm; see ADR 0007.

## Windows GUI and Broker

- Windows 11 development host;
- Microsoft C++ Build Tools / Visual Studio with desktop C++ workload;
- Windows SDK matching the selected toolchain;
- WebView2 runtime;
- Rust MSVC target.

Tauri on Windows still needs Microsoft native build tools even though application logic is Rust and Vue.

## Windows driver

Required only when Gate 5 begins:

- Visual Studio Build Tools or supported Visual Studio edition;
- matching Windows SDK and WDK;
- MSBuild;
- WinDbg;
- isolated test Windows VM or dedicated installation;
- test certificate and test signing inside the target only;
- later EV certificate/Partner Center path for public distribution.

The WDK is not required to work on requirements, Core, Protocol, Runtime, frontend, Android Adapter or application-level audio transport.

## Android

- JDK 21 or a version supported by the selected Tauri/Android Gradle stack;
- Android Studio or command-line SDK tools;
- Android SDK platform and build tools selected by generated project;
- Android NDK selected by generated project;
- ADB;
- physical Android device for microphone tests.

Do not hard-code an Android SDK/NDK version before running `pnpm tauri android init` with the pinned Tauri CLI and recording the generated baseline.

## Environment doctor

Run:

```bash
cargo xtask doctor
```

The command checks executables and reports optional platform tools. It does not install anything or change system security settings.

## Version update process

1. Create an issue describing motivation and risk.
2. Update toolchain manifests in one branch.
3. Regenerate Rust and pnpm lockfiles online.
4. Run Core, UI and platform CI.
5. Review release/security notes.
6. Update this document and an ADR if compatibility or architecture changes.
