# CapyIO

> **Pre-alpha controlled lab. Real IMU, remote-speaker and remote-microphone
> paths now exist, but installation, security and release qualification are not
> complete.**

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

The repository combines deterministic foundation code with three controlled-
lab vertical slices:

- pure Rust domain/runtime/protocol crates;
- typed Port and independent Route lifecycle tests;
- a browser Mock UI and a Tauri host with Quick Actions and Workspace views;
- real Android IMU to Windows Panel/Recorder operation through SensorServer;
- a dedicated Windows `CapyIO Speaker` path to an Android receiver;
- an Android MicYou microphone path into a Windows virtual capture endpoint;
- one first-party Android audio Node shell and bounded native-LAN
  wire/packetizer/worker reference, plus a speaker implementation awaiting
  exact-device audible acceptance;
- Adapter manifests, bounded external-process supervision and local platform
  helpers;
- offline repository validation and CI commands.

The real paths above are accepted only on the identified local lab. CapyIO does
**not** yet provide a production installer, signed public packages, a complete
first-party Android media path, pairing/authenticated transport, WAN support,
camera sharing or a release-qualified background/reboot lifecycle.

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
adapters/audio-share        bounded remote-speaker process boundary
adapters/micyou             bounded remote-microphone process boundary
adapters/native-audio-lan   bounded native audio UDP lab reference
adapters/mock-source        finite test Source Sidecar and manifest
adapters/mock-sink          finite test Sink Sidecar and manifest
platform/windows            Windows Broker, presence and host-config helpers
platform/android            CapyIO Android audio Node/service shell
drivers/windows-audio       CapyIO Speaker/microphone controlled-lab package
protocol/                   Protobuf and JSON Schema sources
docs/                       normative requirements, architecture and plans
```

## Validate and run

Prerequisites and pinned versions are in [docs/TOOLCHAIN.md](docs/TOOLCHAIN.md).

```text
cargo xtask doctor
cargo xtask ci
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
