# CAPY-CAMERA-001B0 Report

Date: 2026-08-29

Status: implementation and available validation complete; changes uncommitted.

Base: CAPY-IO-CONTRACTS-001 at fc3da36

Branch: codex/capyio-camera

## Outcome

The Windows camera crate now has a non-mutating Media Foundation projection
seam in front of the deterministic CAPY-CAMERA-001A source. It fixes the first
Projection to session lifetime, current-user access and one exact NV12
1280x720 30 fps stream.

The slice provides:

- a bounded friendly-name and CapyIO-owned source-CLSID plan;
- a closed configured/started/stopped/shutdown state machine;
- absolute QPC-correlated 100 ns sample timestamps scoped to one stream epoch;
- checked row-wise copy into a positive-stride NV12 buffer with zeroed padding;
- an optional Windows probe that calls only
  `MFIsVirtualCameraTypeSupported(SoftwareCameraSource)`.

## Local prerequisite evidence

The development host reported Windows `10.0.26200.0`. Windows SDK
`10.0.26100.0` provides `mfvirtualcamera.h`, the x64 `mfsensorgroup.lib` import
library and the system `mfsensorgroup.dll`. Visual Studio Build Tools 2022
17.14.39 and Rust 1.97.1 MSVC are installed.

The read-only probe returned:

~~~text
mode=read_only_support_probe
software_camera_supported=true
~~~

This establishes only that the host reports support for the virtual-camera
type. It does not prove COM media-source activation, registration, ordinary-app
enumeration, privacy access, frame delivery or rollback.

## Dependency and provenance

The crate adds an optional, Windows-only direct dependency on Microsoft
`windows` 0.61.3 with only `Win32_Media_MediaFoundation`. The dependency is MIT
OR Apache-2.0, locked by Cargo.lock and recorded in `third_party/THIRD_PARTY.yml`.
No Windows-Camera sample source or binary was imported. An attempt to query the
sample repository HEAD was interrupted by a reset network connection, so the
sample remains an unpinned documentation/design reference only.

## Safety boundary

The implementation contains no `MFCreateVirtualCamera`, `Start`, registration,
enumeration, stop/removal, COM class server, driver, camera-device read, codec
or network path. The probe is opt-in and read-only. No system state was changed.
Any later registration/start or removal command requires separate explicit
human approval and an exact rollback plan.

## Validation

The following focused commands completed successfully:

~~~text
cargo fmt --all
cargo test -p capyio-windows-camera
cargo clippy -p capyio-windows-camera --all-targets --all-features -- -D warnings
cargo check -p capyio-windows-camera --features windows-mf-probe --all-targets
cargo run -p capyio-windows-camera --features windows-mf-probe --bin capyio-camera-mf-probe
~~~

The crate has 13 passing tests: seven fixture/queue tests and six Media
Foundation projection tests. The following repository-wide checks also
completed successfully:

~~~text
cargo check --workspace
cargo xtask validate-docs
cargo xtask validate-manifests
cargo xtask ci
git diff --check
~~~

The CI gate passed workspace formatting, check, Clippy, tests/doc-tests,
deterministic IMU demo, document/manifest/structure validation, Adapter smoke
and crash isolation, desktop TypeScript checking and the production UI build.

## Deferred work

- implement a minimal Frame Server-compatible COM media source and class server;
- implement a registrar without invoking it during build/test;
- define exact start/stop/shutdown ownership and failure injection tests;
- obtain separately approved registration, enumeration, frame and cleanup
  evidence from an isolated/current-user lab run.
