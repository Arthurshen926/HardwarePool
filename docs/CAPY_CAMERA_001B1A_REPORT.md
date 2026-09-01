# CAPY-CAMERA-001B1A Report

Date: 2026-08-29

Status: implementation and available validation complete; changes uncommitted.

Base: CAPY-IO-CONTRACTS-001 at fc3da36

Branch: codex/capyio-camera

## Outcome

The camera crate now has deterministic protocol cores for the one-stream Frame
Server media source and its future registration owner. This narrows the next
Windows COM slice to translating tested actions into Media Foundation objects
and event queues.

The media-source core:

- admits only the canonical single selected stream ID 0;
- emits new-stream, stream-started and source-started actions in official order;
- emits updated-stream after a stop/restart and advances a stream generation;
- holds at most four outstanding sample-request tickets in fixed storage;
- completes tickets FIFO with advancing frame sequence, transactionally;
- cancels every outstanding request on stop or terminal shutdown.

The registrar core:

- makes prepare/start/stop/shutdown distinct backend operations;
- rejects invalid transitions without invoking its backend;
- attempts immediate shutdown after a start failure;
- attempts terminal shutdown even if stop fails;
- exposes `CleanupRequired` when both an operation and rollback fail, allowing
  explicit cleanup retry.

## Official behavior reviewed

The implementation follows Microsoft's SimpleMediaSource single-stream event
order and `IMFMediaStream::RequestSample` pull model. The official sample and
documentation are references only; no source or binary was imported.

## Safety boundary

CAPY-CAMERA-001B1A contains no Windows COM implementation and no Windows
registration backend. Registrar tests use only an in-memory fake. No call to
`MFCreateVirtualCamera`, `IMFVirtualCamera::Start`, stop, remove or shutdown was
made against the operating system, and no system camera state changed.

## Validation

The focused commands completed successfully:

~~~text
cargo fmt --all
cargo test -p capyio-windows-camera
cargo clippy -p capyio-windows-camera --all-targets --all-features -- -D warnings
~~~

The camera crate now has 26 passing tests: seven fixture/queue, six projection,
seven media-source protocol and six registrar rollback tests. These additional
checks also completed successfully:

~~~text
cargo check --workspace
cargo check -p capyio-windows-camera --features windows-mf-probe --all-targets
cargo xtask validate-docs
cargo xtask validate-manifests
cargo xtask ci
git diff --check
~~~

The full CI gate passed workspace formatting, check, Clippy, tests/doc-tests,
deterministic IMU demo, document/manifest/structure validation, Adapter smoke
and crash isolation, desktop TypeScript checking and the production UI build.

## Deferred work

- implement `IMFMediaSourceEx`/`IMFMediaStream2` with separate Media Foundation
  event queues and one exact NV12 media type;
- map each accepted sample request to one generated frame, 2D MF buffer and
  `MEMediaSample` event without holding a contended lock in the callback;
- add a local non-registered COM activation harness before any system catalog
  operation;
- add the Windows registration backend and controlled app-enumeration evidence
  only under separate explicit approval.
