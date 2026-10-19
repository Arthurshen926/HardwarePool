# CapyIO

Cloud source snapshot: [2026-10-19 handoff and validation](docs/CLOUD_SYNC_HANDOFF_20261019.md).

Local installable candidate: [delivery and limits](docs/LOCAL_ALPHA_CURRENT.md).
Windows users: [first run](docs/FIRST_RUN_WINDOWS.md).

Current integration status: [CURRENT_STATUS](docs/CURRENT_STATUS.md).
Architecture navigation: [CURRENT_ARCHITECTURE](docs/CURRENT_ARCHITECTURE.md).
The [five-phase Alpha closeout](docs/ALPHA_CONSOLIDATION.md) freezes new hardware
features and separates software checks from physical and release acceptance.

> **0.5.24 Alpha Release Preparation — feature frozen.** Windows + Android/Y700
> has bounded physical acceptance; public distribution and driver signing are not complete.

**CapyIO — Cross-device I/O Capability Fabric**
**CapyIO 跨设备 I/O 能力织网平台**

CapyIO connects I/O already built into phones, tablets, laptops, desktops and
embedded devices. A Node publishes user-understandable Capabilities; each
Capability owns typed Source, Sink or Control Ports; a Route connects compatible
Ports. When a platform supports it, an Adapter can expose a remote capability as
a system virtual device. Otherwise it can use an API, built-in Panel, standard
protocol or recording output.

CapyIO is not remote desktop, USB/IP with a GUI, one universal media transport,
or a collection of copied vertical tools. Existing projects may be integrated
behind isolated Adapter boundaries while the Core owns identity, catalogs,
Routes, lifecycle and diagnostics.

## Current executable scope

The current primary path is first-party pairing, authenticated control, catalogs,
native speaker/microphone, camera and touchpad. Windows UI and Node Host are
separate processes: closing the window leaves the node running; reopening attaches
to it. Android uses one service with capability-specific modules.

Gamepad, haptics, display and IMU remain experimental. Audio Share, MicYou and
SensorServer are retained compatibility integrations, not default runtime
prerequisites. Their sources and developer diagnostics remain available.

The tested environment is Windows + Android/Y700 on trusted LAN/Tailscale.
Control TLS does not imply full application-layer media encryption. Camera
projection currently requires an elevated Host. System audio, VHF touchpad,
camera registration and optional display/gamepad backends have installation
prerequisites; the application installer does not deploy them automatically.
No public signed driver package or production installer is claimed.

See [current evidence](docs/ALPHA_FOLLOWUP_20261003.md),
[system component model](docs/SYSTEM_COMPONENTS.md) and
[release preparation](docs/ALPHA_RELEASE_PREPARATION.md).

## Repository map

```text
crates/capyio-core          portable domain rules
crates/capyio-audio         reusable audio frame/buffer primitives
crates/capyio-protocol      capyio.v1 control protocol
crates/capyio-runtime       deterministic route/catalog orchestration
crates/capyio-adapter-sdk   Adapter manifest and control contract
crates/capyio-adapter-host  desktop sidecar supervision
crates/capyio-testkit       stable fixtures
apps/capyio-node            headless four-Route demo
apps/desktop                Vue + Tauri control surface
adapters/audio-share        compatibility-only remote-speaker boundary
adapters/micyou             compatibility-only remote-microphone boundary
adapters/native-audio-lan   bounded native audio UDP lab reference
adapters/mock-source        finite test Source Sidecar and manifest
adapters/mock-sink          finite test Sink Sidecar and manifest
platform/windows            Windows Broker, presence and host-config helpers
platform/android            unified Android Node and capability modules
drivers/windows-audio       CapyIO Speaker/microphone controlled-lab package
protocol/                   Protobuf and JSON Schema sources
docs/                       normative requirements, architecture and plans
```

## Validate and run

Prerequisites and pinned versions are in [docs/TOOLCHAIN.md](docs/TOOLCHAIN.md).

```text
cargo xtask doctor
cargo xtask ci
cargo xtask alpha-check
cargo xtask demo
cargo xtask adapter-smoke
cargo xtask android-check
pnpm dev
```

`cargo xtask android-check` builds and Lints an uninstalled debug APK without
ADB or device mutation. `cargo xtask demo` and the browser-only UI remain
explicitly simulated. The
Tauri shell can expose real lab Quick Actions when their separately approved
host dependencies are installed and configured. See
[First Run on Windows](docs/FIRST_RUN_WINDOWS.md) and
[Android microphone sharing](docs/MICROPHONE_SHARING_WINDOWS_ANDROID.md).
For the current unified Android/Windows physical test order, UI map and
per-capability pass/fail criteria, use the
[physical acceptance guide](docs/PHYSICAL_ACCEPTANCE_GUIDE.md).

## Product modes

- **Quick Actions** present tasks such as “Use phone as microphone”.
- **Workspace / Lab** exposes Nodes, Capabilities, Ports, Routes, Adapters and
  Problems for developers and research users.

## Safety

Ordinary validation commands do not install a driver or APK. Driver/APK
deployment and permission changes remain separately approved operations under
the repository safety rules. Microphone capture requires visible,
platform-compliant permission and lifecycle handling.

## License

CapyIO source remains Apache-2.0. Third-party vertical programs stay behind
recorded process boundaries and keep their own licenses; in particular, the
locally patched GPL-3.0-only MicYou executable is not distributed by CapyIO.
Provenance, pins and local modifications are tracked in
`third_party/THIRD_PARTY.yml`.
