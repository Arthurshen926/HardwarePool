# CAPY-CAMERA-001B1B Report

Date: 2026-08-29

Status: implementation and available validation complete; changes uncommitted.

Base: CAPY-IO-CONTRACTS-001 at fc3da36

Branch: codex/capyio-camera

## Outcome

The new Windows-only `capyio-windows-camera-mf` crate projects the tested camera
contracts as a directly constructed, one-stream Media Foundation COM source.
It implements `IMFMediaSourceEx` and `IMFMediaStream2` without a class factory,
DLL export, registry write or virtual-camera registration backend.

The implementation provides:

- one exact NV12 1280x720 30 fps progressive media type and canonical stream 0;
- mandatory Frame Server stream ID, video-capture category, shared-stream and
  color-source attributes;
- separate thread-safe source and stream event queues;
- first-start `MENewStream`, restart `MEUpdatedStream`, stream/source
  start/stop events and terminal idempotent shutdown;
- a strong source-to-stream and weak stream-to-source ownership graph without a
  COM reference cycle;
- a non-blocking `RequestSample` lock, fixed FIFO ticket and one deterministic
  `MFCreate2DMediaBuffer`/`IMFSample` per accepted pull;
- QPC-correlated 100 ns time/duration, caller-token preservation and cleared
  NV12 stride padding;
- transactional ticket cancellation when sample construction fails, leaving a
  started stream recoverable.

## In-process evidence

The Windows integration test initializes COM MTA and Media Foundation, creates
the source and stream directly, and verifies attributes, the exact media type,
event ordering, announced stream identity, token round-trip, first luma byte,
sample buffer length, 30 fps rational timestamp delta, pause/resume, stop,
restart and shutdown behavior. All event reads are non-blocking.

The source owns its stream and the stream keeps only a `windows-core` weak COM
reference. Rust-shared state contains synchronized native state only; COM
interface wrappers remain owned by their source/stream objects. Microsoft's
thread-safe `IMFMediaEventQueue` helper backs each independent event channel.

## Dependency review

This slice expands the existing Microsoft windows-rs dependency record to the
locked `windows` 0.61.3 and direct `windows-core` 0.61.2 packages. Enabled Win32
features are limited to Foundation, Kernel Streaming, Media Foundation, COM,
Structured Storage and Variant. Both packages are MIT OR Apache-2.0. They avoid
hand-written COM/PROPVARIANT declarations. No Windows-Camera sample source or
binary was imported.

## Safety boundary

The crate imports and calls no `MFCreateVirtualCamera` or `IMFVirtualCamera`
API. It exposes no activation class outside the process and cannot register,
enumerate or start a system camera. No camera device was opened, no personal
image was captured and no operating-system camera state changed.

## Validation

Focused implementation checks completed successfully:

~~~text
cargo fmt --all
cargo test -p capyio-windows-camera
cargo test -p capyio-windows-camera-mf -- --nocapture
cargo clippy -p capyio-windows-camera-mf --all-targets -- -D warnings
~~~

The pure camera crate has 27 passing tests and the COM crate has one passing
Windows integration test. The full `cargo xtask ci` gate also passed workspace
formatting, check, Clippy, tests/doc-tests, deterministic IMU demo,
document/manifest/structure validation, Adapter smoke and crash isolation,
desktop TypeScript checking and the production UI build.

## Deferred work

- provide a reviewed class-activation boundary that Windows Frame Server can
  load without exposing installation or registration through normal tests;
- implement the session/current-user `IMFVirtualCamera` backend behind the
  existing registrar core;
- run registration, enumeration, application frame delivery and rollback only
  as separately approved exact lab commands;
- replace the deterministic fixture with a reviewed camera Adapter/data plane
  in a later slice; no phone or hardware capture belongs to 001B1B.
