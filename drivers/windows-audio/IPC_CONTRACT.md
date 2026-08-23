# Broker ↔ Windows Audio Driver IPC Contract

> Status: design constraint, not a frozen binary ABI  
> Version: bootstrap-0

## 1. Purpose

The IPC surface transfers bounded PCM blocks and minimal endpoint state between the Windows user-mode Broker and the virtual audio driver. It does not carry CapyIO network messages or Core objects.

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
| `QUERY_INTERFACE` | Broker → Driver | Read interface version, limits and supported features |
| `ATTACH_RENDER_RING` | Broker → Driver | Attach the bounded render PCM ring |
| `ATTACH_CAPTURE_RING` | Broker → Driver | Attach the bounded capture PCM ring |
| `SET_ENDPOINT_STATE` | Broker → Driver | Mark remote endpoint online/offline/muted |
| `QUERY_COUNTERS` | Broker → Driver | Read underrun, overrun, dropped-block and generation counters |
| `DETACH` | Broker → Driver | End the current Broker generation cleanly |

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

The exact Windows mechanism—buffered/direct IOCTL, section-backed shared memory, event objects, KMDF/WDM integration—is intentionally deferred until the SysVAD spike. Freezing it now would be premature.
