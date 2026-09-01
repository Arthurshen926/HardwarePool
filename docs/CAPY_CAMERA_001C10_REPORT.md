# CAPY-CAMERA-001C10 — Windows camera stage-latency evidence

Date: 2026-08-30

Status: complete for bounded decoder and Local shared-publication observation.

## Objective

Add same-clock measurements for the Windows portion of the live camera path
without changing the CAVC wire contract or comparing unrelated Android and
Windows monotonic clocks. This slice does not measure capture, Android encoding,
network residence, Frame Server consumption or display latency.

## Implementation

- Each accepted Windows decoder sample retains a bounded `Instant` alongside
  its existing sequence/timestamp metadata.
- When the matching NV12 output has been copied, `StageLatencyStats` records a
  saturating sample count, total microseconds and maximum microseconds. Average
  is derived without retaining per-frame observations.
- The Local/Global shared-frame publisher separately measures conversion plus
  shared-memory publication using the same Windows monotonic clock.
- The loopback receiver emits sample count, average and maximum for both stages
  in its final bounded summary. No raw frame or unbounded timing series is kept.

## Automated evidence

`cargo test -p capyio-vcamdroid-adapter --all-targets` passed 12 tests, including
the deterministic latency accumulator contract and the inbox H.264 low-latency
mode check. Warnings-denied Clippy and `git diff --check` passed.

## Physical evidence

The existing authorized Camera Lab APK on `V2419A` was used through a temporary
ADB reverse mapping. No APK install, permission change, virtual-camera
registration, DLL deployment or administrator action occurred.

Decode-only stream `93b54d916940196e7375e05b5db2de8d`:

- 300 access units and 300 decoded 1280x720 NV12 frames;
- distinct first/last checksums;
- Windows decoder low-latency mode enabled;
- maximum pending decoder samples: 0;
- submit-to-copied-NV12 average: 2,413 microseconds;
- submit-to-copied-NV12 maximum: 7,682 microseconds.

Local shared-publication stream `bd5f34dc3fd54824a4fcee02abe3eb79`:

- 300 access units, 300 decoded frames and 300 Local publications;
- distinct first/last checksums;
- maximum pending decoder samples: 0;
- submit-to-copied-NV12 average: 2,399 microseconds;
- submit-to-copied-NV12 maximum: 7,528 microseconds;
- NV12 conversion plus shared publication average: 137 microseconds;
- NV12 conversion plus shared publication maximum: 521 microseconds.

These samples indicate that the measured Windows decoder and shared-publication
stages are small relative to the previously observed human-visible delay. This
is a bounded diagnostic inference, not proof that another named stage is the
cause and not an end-to-end latency claim.

## Cleanup and next boundary

The app was force-stopped after each run. Final checks found no ADB reverse
mapping, active Android camera client or receiver process. The temporary UI XML
used to locate the foreground button was deleted.

Next measurement should instrument Android encoder-output-to-socket residence
within the Android clock domain, followed by the existing clock-in-camera
glass-to-glass test. Reconnect and long-duration stability remain separate
slices.
