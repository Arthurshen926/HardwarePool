# CAPY-CAMERA-001A Report

Date: 2026-08-29

Status: implementation and available validation complete; changes uncommitted.

Base: CAPY-IO-CONTRACTS-001 at fc3da36

Branch: codex/capyio-camera

## Scope

This slice creates a deterministic, hardware-free 1280x720 30 fps NV12 source
and a bounded owned-frame queue in capyio-windows-camera. It is a generated
data-plane fixture for exercising the Video contract and later projection
seams.

The source:

- renders limited-range NV12 color bars plus a moving marker and binary
  sequence clock;
- derives timestamps from sequence and a rational 30 fps timebase without
  reading the system clock;
- validates every descriptor and exact packed payload length;
- exposes an FNV-1a diagnostic checksum for detecting fixture drift.

The queue:

- accepts only validated frames for the canonical fixture VideoStreamSpec;
- owns at most 12 frames / 16,588,800 payload bytes;
- supports explicit reject-newest or drop-oldest overflow policy;
- marks the next retained frame discontinuous after dropping an older frame;
- records bounded monotonic diagnostics and a high-water mark.

## Safety boundary

There are no Media Foundation calls, virtual-camera registration/removal,
driver operations, Android device reads, personal video fixtures, codecs,
network paths, wall-clock dependencies or control-path frame bytes. The
checksum is diagnostic only and is not authentication or a security integrity
mechanism.

## Validation

The following commands completed successfully in the Camera worktree:

~~~text
cargo test -p capyio-windows-camera
cargo clippy -p capyio-windows-camera --all-targets -- -D warnings
cargo fmt --all -- --check
cargo check --workspace
cargo xtask validate-docs
cargo xtask ci
git diff --check
~~~

The focused Camera suite has seven passing tests. It pins the first-frame
checksum, verifies one-second rational timestamp alignment, validates packed
NV12 bounds and exercises reject-newest/drop-oldest overflow behavior.

The full CI run also passed workspace Clippy/tests, the deterministic IMU demo,
documentation and manifest validation, Adapter smoke/crash isolation, desktop
TypeScript checking and the desktop production build.

## Deferred work

- Media Foundation feasibility and generated-frame projection require a
  dedicated platform ADR and official SDK/tool evidence.
- Any virtual-camera registration or removal remains a separately approved
  system mutation.
- Android Camera2 inventory/capture and VCamdroid Adapter work are outside this
  slice.
