# CapyIO Toolchain

> Foundation baseline: 2026-08-23. Update pins through an explicit toolchain
> task, lockfile regeneration and validation.

## required-now

- Git 2.4x+;
- Rust/rustc/cargo 1.97.1 with rustfmt and Clippy;
- Node.js 24 LTS (repository accepts `>=24 <27`);
- Corepack and pnpm 11.5.3 from root `packageManager`;
- Python 3.11+;
- MSVC/Windows SDK and WebView2 for the Tauri desktop shell on Windows;
- vendored `protoc` supplied by `protoc-bin-vendored`.

Foundation frontend pins include Tauri CLI 2.11.2, Tauri JavaScript API
2.11.1, Vue 3.5.35, TypeScript 6.0.3 and Vite 8.0.16. The CLI/API version
difference is recorded in ADR 0007.

## optional-android

- Android Studio;
- compatible JBR/JDK, Android SDK command-line tools, platform/build tools, NDK;
- Rust Android targets;
- ADB and an explicitly designated physical device.

The current machine has Android Studio 2026.1.3.7 and JBR 25.0.2, plus the
command-line-tools shell under the user SDK directory. Foundation Gates 0–3 do
not require SDK packages, ADB, Android project generation, APK installation or
permissions, so no additional Android component is installed by this task.

## optional-windows-native

- Visual Studio/Build Tools desktop C++ workload;
- Windows SDK matching the selected host target;
- MSBuild when a native helper requires it.

The Windows Rust/Tauri workspace compiles on the current host. `msbuild` is not
visible on the audited session PATH and is not required for pure foundation
tests.

## optional-driver

- supported Visual Studio, Windows SDK and WDK;
- MSBuild, WinDbg and signing/test tools;
- isolated Windows VM or dedicated installation.

These are deliberately absent from foundation requirements. The WDK and driver
tools must never be auto-installed or executed on the daily-development host.

## optional-media

FFmpeg, GStreamer, AOO, WebRTC stacks, codec SDKs, ROS 2, MCAP and USB/IP tools
are future Adapter dependencies, not Core prerequisites.

## Windows session PATH note

The current Codex process did not inherit newly installed tool locations. Until
a fresh shell is opened, local validation prefixes:

```powershell
$env:Path = 'C:\Users\arthu\.cargo\bin;C:\Program Files\nodejs;' + $env:Path
```

Use `corepack pnpm` so the root `packageManager` pin is honored. A populated
`node_modules` created by another pnpm version may require one
`corepack pnpm install --frozen-lockfile` rebuild.

## Doctor categories

`cargo xtask doctor` reports the categories above separately. Missing optional
Android, Windows-native, driver or media tools does not fail Core development.
The command is read-only and never installs or reconfigures the machine.
