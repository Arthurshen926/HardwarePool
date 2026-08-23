# CapyIO

> **Pre-alpha foundation. No real hardware Adapter, system virtual device or
> production network path is connected yet.**

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

The repository currently provides deterministic foundation code only:

- pure Rust domain/runtime/protocol crates;
- typed Port and independent Route lifecycle tests;
- a browser/Tauri Mock UI with Quick Actions and Workspace views;
- an Adapter manifest and local sidecar-control test path;
- offline repository validation and CI commands.

It does **not** capture a phone microphone, play remote audio, open a camera,
read IMU hardware, install a driver, create a Windows endpoint, pair devices or
secure a network connection.

## Repository map

```text
crates/capyio-core          portable domain rules
crates/capyio-audio         reusable audio frame/buffer primitives
crates/capyio-protocol      capyio.v1 control protocol
crates/capyio-runtime       deterministic route/catalog orchestration
crates/capyio-adapter-sdk   Adapter manifest and control contract
crates/capyio-adapter-host  desktop sidecar supervision
crates/capyio-testkit       stable fixtures
apps/capyio-node            headless demo and mock sidecars
apps/desktop                Vue + Tauri control surface
adapters/                   integration boundaries and mock manifests
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
pnpm dev
```

`cargo xtask demo` and the UI are explicitly simulated. Metrics and Route state
do not represent physical devices.

## Product modes

- **Quick Actions** present tasks such as “Use phone as microphone”.
- **Workspace / Lab** exposes Nodes, Capabilities, Ports, Routes, Adapters and
  Problems for developers and research users.

## Safety

No repository command installs a driver or APK. Windows driver testing must use
an isolated VM or dedicated machine. Microphone capture requires visible,
platform-compliant permission and lifecycle handling.

## License

The foundation remains Apache-2.0. No third-party vertical-project source is
vendored in the current Gates. Future integration provenance and licenses are
tracked in `third_party/THIRD_PARTY.yml` before code import.
