# Broker ↔ Windows Audio Projection IPC Contract

> Status: implemented Gate 7B lab ABI; not a production-stable ABI
> Version: 1

## 1. Purpose

This IPC transfers bounded render PCM blocks between the user-mode render APO
and Broker. It carries no CapyIO network messages, Core objects, identities,
addresses or codec packets. ADR 0028 selects this pre-opened SPSC staging ring;
a direct driver ring remains a fallback.

## 2. Design constraints

- Every layout has a magic, version, total size and fixed bounds.
- All counts, offsets and arithmetic are validated before copying.
- The APO callback performs only bounded copy/scale, atomics and drop accounting.
- Broker or phone absence degrades to drop/silence and never blocks AudioDG.
- A generation prevents stale blocks entering a new Broker lifetime.

## 3. Render ring v1

The elevated lab Broker creates `Global\\CapyIO.RenderRing.v1` before Windows opens a render
stream. The post-mix MFX APO opens it during `LockForProcess`; it never creates, opens or
maps objects from `APOProcess`. Both sides require 48,000 Hz, stereo,
interleaved IEEE float32 little-endian input. A format mismatch leaves the APO
detached.

The mapping is 524,928 bytes: one 128-byte, 64-byte-aligned header followed by
32 fixed 16,400-byte slots. Each slot contains a 16-byte prefix and at most
16,384 payload bytes. All offsets and integers are little-endian.

The APO subscribes to the virtual endpoint's Windows master-volume and mute
notifications outside `APOProcess`. It stores the current normalized gain as
one fixed-point atomic value. `APOProcess` snapshots that value and applies it
only while copying the block into the ring: mute writes zeroes, unity uses the
bounded copy path, and intermediate volume performs one bounded per-sample
multiply. The Windows audio-engine buffer is not modified, and notification,
COM and endpoint queries never execute on the real-time callback.

| Header offset | Type | Field |
|---:|---|---|
| 0 | `u32` | magic `0x524f4950` |
| 4 | `u16` | version `1` |
| 6 | `u16` | header size `128` |
| 8 | `u32` | total mapping size `524928` |
| 12 | `u32` | slot count `32` |
| 16 | `u32` | slot stride `16400` |
| 20 | `u32` | payload capacity `16384` |
| 24 | `u32` | sample rate `48000` |
| 28 | `u16` | channels `2` |
| 30 | `u16` | sample format `1` (float32 LE) |
| 32 | `u64` | Broker generation |
| 40 | atomic `i64` | producer write sequence |
| 48 | atomic `i64` | consumer read sequence |
| 56 | atomic `i64` | dropped block count |
| 64 | atomic `i64` | produced block count |
| 72 | atomic `i64` | non-real-time APO attach attempts after the mapping is opened and mapped |
| 80 | atomic `i64` | successful validated APO attaches |
| 88 | atomic `u32` | last observed sample rate |
| 92 | atomic `u32` | last observed channel count |
| 96 | atomic `u32` | last attach stage: `1` mapped, `2` validation failed, `3` attached |
| 100 | atomic `u32` | last Win32-style attach error (`13` means invalid layout/format) |
| 104 | 24 bytes | reserved, zero |

The attach fields are bounded lab diagnostics written only from
`LockForProcess`, never from `APOProcess`. All-zero attach fields mean that the
APO did not reach the mapped-view stage; they do not distinguish an absent
`LockForProcess` call from an object-open or view-map failure.

Each slot prefix is `generation: u64`, `byte_count: u32`, and
`frame_count: u32`. The producer writes prefix and payload, executes a release
barrier, then advances `write_sequence`. The consumer acquire-loads that
sequence, validates generation and exact `frame_count * channels * 4`, copies
one committed block, then release-stores `read_sequence`. Invalid layouts stop
the Broker; a missing/full ring only increments the APO drop counter. Float32
to S16LE conversion and network submission occur only in the Broker.

## 4. PCM epoch contract

One mapping lifetime is one format epoch: sample rate, representation, channel
count/layout and generation. Changing format requires a new generation. A
block from an old generation is rejected.

## 5. Capture frame ring v1

ADR 0037 selects paired Windows endpoints for MicYou compatibility. The
CapyIO service creates `Global\\CapyIO.CaptureRing.v1`. The MFX on `CapyIO
Microphone Ingress` is the sole producer; the MFX on `CapyIO Microphone` is the
sole consumer. Both open and validate the mapping during `LockForProcess`, not
from `APOProcess`.

The mapping is 65,664 bytes: one 128-byte, 64-byte-aligned header followed by
16,384 mono float32 frames. At 48 kHz this is an absolute capacity bound of
341.33 ms, not a target latency. Monotonic frame sequences allow producer and
consumer callback sizes to differ without allocating or repacketizing.

| Header offset | Type | Field |
|---:|---|---|
| 0 | `u32` | magic `0x434f4950` |
| 4 | `u16` | version `1` |
| 6 | `u16` | header size `128` |
| 8 | `u32` | total mapping size `65664` |
| 12 | `u32` | frame capacity `16384` |
| 16 | `u32` | bytes per frame `4` |
| 20 | `u32` | sample rate `48000` |
| 24 | `u16` | channels `1` |
| 26 | `u16` | sample format `1` (float32 LE) |
| 28 | `u32` | reserved, zero |
| 32 | `u64` | service generation |
| 40 | atomic `i64` | producer write-frame sequence |
| 48 | atomic `i64` | consumer read-frame sequence |
| 56 | atomic `i64` | dropped ingress frames |
| 64 | atomic `i64` | produced frames |
| 72 | atomic `i64` | consumed frames |
| 80 | atomic `i64` | zero-filled underrun frames |
| 88 | atomic `i64` | producer attach attempts |
| 96 | atomic `i64` | successful producer attaches |
| 104 | atomic `i64` | consumer attach attempts |
| 112 | atomic `i64` | successful consumer attaches |
| 120 | atomic `u32` | last attach stage (`101`/`102`/`103` producer, `201`/`202`/`203` consumer) |
| 124 | atomic `u32` | last Win32-style attach error |

The ingress APO accepts 48 kHz float processing buffers. Mono is copied and
stereo is downmixed with equal `0.5` weights. It commits a complete callback or
drops the complete callback when capacity is insufficient. The capture APO
copies up to the requested mono frame count, zero-fills the remainder and
advances only by frames actually consumed. A detached capture APO therefore
returns silence instead of exposing the underlying SysVAD test tone.

The ring carries live microphone frames, not a recording backlog. During the
non-real-time consumer attach, the capture APO atomically synchronizes the read
sequence to the current write sequence before processing starts. Frames that
accumulated while no capture application was active are therefore discarded;
a newly opened application receives only frames produced after its attach and
cannot replay stale speech after an earlier phone disconnect.

## 6. Offline behavior

- The render endpoint continues accepting Windows audio and drops/counts when
  the Broker is absent or the bounded ring is full.
- Network and Android receiver state never enter the APO or kernel driver.
- Driver unload, Broker exit and process crash must not wait on an audio callback.
- MicYou or ingress loss drains an already-active capture session and then
  produces silence. A newly attached capture session skips any pre-attach
  backlog; the capture callback never waits for new frames.

## 7. Deferred production decisions

The lab mapping uses a protected DACL granting read/write to Local Service
(AudioDG), full access to Local System, administrators and the object owner,
and no inheritance.
Creating the global object from an interactive session requires an elevated
Broker. A production ABI still needs an authenticated setup/control channel,
least-privilege service identity, wake-up strategy, format renegotiation and
compatibility policy. Buffered/direct IOCTL or a driver-owned section remains
a fallback, not the default.
