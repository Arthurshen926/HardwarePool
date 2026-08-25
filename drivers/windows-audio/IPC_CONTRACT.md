# Broker ↔ Windows Audio Projection IPC Contract

> Status: deferred fallback design, not a frozen binary ABI
> Version: bootstrap-0

## 1. Purpose

The primary IPC surface transfers bounded render PCM blocks and minimal
endpoint state between the user-mode render APO and Broker. It does not carry
CapyIO network messages or Core objects. ADR 0028 selects a pre-opened bounded
shared-memory/SPSC staging ring for the first `CapyIO Speaker` spike. A direct
driver ring remains a fallback if the APO lifecycle or certification evidence
fails.

## 2. Design constraints

- Version every request and shared-memory header.
- Use an explicit magic value, total structure size and feature bitset.
- Validate all counts, offsets and arithmetic before mapping or copying.
- Use fixed upper bounds for ring size, block size, channels and outstanding operations.
- Define little-endian integer fields explicitly; do not expose compiler-dependent Rust/C struct layout across the boundary.
- Keep render and capture rings independent.
- Use generation/epoch counters so stale Broker data cannot enter a new connection.
- Driver unload, Broker exit and process crash must close handles without blocking audio-engine callbacks.

## 3. Bootstrap message families

| Operation | Direction | Meaning |
|---|---|---|
| `QUERY_INTERFACE` | Broker → APO companion | Read interface version, limits and supported features |
| `ATTACH_RENDER_RING` | Broker → APO companion | Establish the bounded render PCM ring before real-time processing |
| `ATTACH_CAPTURE_RING` | Broker → Driver | Attach the bounded capture PCM ring |
| `SET_ENDPOINT_STATE` | Broker → APO companion | Mark remote endpoint online/offline/muted outside the real-time callback |
| `QUERY_COUNTERS` | Broker → APO companion | Read underrun, overrun, dropped-block and generation counters |
| `DETACH` | Broker → APO companion | End the current Broker generation cleanly |

No operation may contain a hostname, IP address, certificate, peer identity, Protobuf payload or codec packet.

## 4. PCM epoch contract

A ring attachment is valid for exactly one negotiated PCM epoch:

- sample rate;
- sample representation;
- channel count and layout;
- frame/block size;
- generation ID.

Changing format requires a new generation. A block from an old generation is discarded.

## 5. Offline behavior

- Render endpoint: continue accepting Windows audio according to the Windows driver model, count/drop blocks, and report the remote sink offline.
- Capture endpoint: provide deterministic silence and report the remote source offline.
- Never wait indefinitely for the Broker or network.

## 6. Deferred decisions

The exact section-handle ownership, non-real-time setup channel and wake-up
mechanism are intentionally deferred until the APO spike. The real-time path
is fixed to a bounded non-blocking copy/drop contract. Buffered/direct IOCTL or
a driver-owned section is a fallback, not the default.
