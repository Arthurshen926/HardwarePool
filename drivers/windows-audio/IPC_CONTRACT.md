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
- The APO callback performs only bounded copy, atomics and drop accounting.
- Broker or phone absence degrades to drop/silence and never blocks AudioDG.
- A generation prevents stale blocks entering a new Broker lifetime.

## 3. Render ring v1

The Broker creates `Local\\CapyIO.RenderRing.v1` before Windows opens a render
stream. The SFX APO opens it during `LockForProcess`; it never creates, opens or
maps objects from `APOProcess`. Both sides require 48,000 Hz, stereo,
interleaved IEEE float32 little-endian input. A format mismatch leaves the APO
detached.

The mapping is 524,928 bytes: one 128-byte, 64-byte-aligned header followed by
32 fixed 16,400-byte slots. Each slot contains a 16-byte prefix and at most
16,384 payload bytes. All offsets and integers are little-endian.

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
| 72 | 56 bytes | reserved, zero |

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

## 5. Offline behavior

- The render endpoint continues accepting Windows audio and drops/counts when
  the Broker is absent or the bounded ring is full.
- Network and Android receiver state never enter the APO or kernel driver.
- Driver unload, Broker exit and process crash must not wait on an audio callback.

## 6. Deferred production decisions

The lab uses a Broker-owned named mapping and bounded 2 ms Broker polling. A
production ABI still needs an authenticated setup/control channel, explicit
security descriptor, wake-up strategy, format renegotiation, session-boundary
review and compatibility policy. Buffered/direct IOCTL or a driver-owned
section remains a fallback, not the default.
