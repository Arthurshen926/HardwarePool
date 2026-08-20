# HardwarePool Audio Profile v1

> Profile names: `hardwarepool.audio.capture/1`, `hardwarepool.audio.render/1`, `hardwarepool.audio.duplex_bundle/1`

## 1. Scope

This document defines the capability metadata and negotiation semantics for the first audio Profiles. It does not select a codec or network transport and does not define a Windows driver ABI.

## 2. Capability roles

### Capture

```text
local_role: capture
stream_role: producer
supported projections:
  - application_stream
  - system_capture_endpoint
```

Typical instance: Android built-in microphone projected as a Windows recording endpoint.

### Render

```text
local_role: render
stream_role: consumer
supported projections:
  - application_stream
  - system_render_endpoint
```

Typical instance: Android built-in speaker projected as a Windows playback endpoint.

### Duplex bundle

The bundle references one capture and one render capability. It may describe a shared acoustic environment and supported processing relationship, but it has no independent audio payload and does not merge permissions.

## 3. Audio format

A negotiated format contains:

- sample rate in Hz;
- sample representation: `signed_i16_le`, future `signed_i24_le`, `signed_i32_le`, `float_f32_le`;
- channel count;
- channel layout;
- frame duration in microseconds;
- interleaving mode (v1 baseline is interleaved);
- codec binding selected separately from the raw media format.

Validation limits in the bootstrap Core:

- sample rate: 8,000–384,000 Hz;
- channels: 1–32;
- frame duration: 2,500–60,000 microseconds.

These limits protect parsers and buffers; a capability may advertise a much smaller supported set.

## 4. MVP baseline

| Direction | Rate | Format | Channels | Frame duration |
|---|---:|---|---:|---:|
| Windows → Android speaker | 48,000 Hz | signed i16 little-endian | 2 | 10 ms |
| Android microphone → Windows | 48,000 Hz | signed i16 little-endian | 1 | 10 ms |

A 10 ms frame contains 480 samples per channel at 48 kHz.

## 5. QoS modes

### `media_playback`

- quality and continuity prioritized over interaction latency;
- stereo expected;
- larger adaptive jitter target allowed;
- AEC normally disabled.

### `voice_interactive`

- latency prioritized while maintaining intelligibility;
- microphone commonly mono;
- AEC/NS/AGC capability negotiation enabled;
- bounded jitter target.

### `raw_lan`

- uncompressed PCM reference and diagnostics mode;
- intended for trusted local testing;
- not a production security or bandwidth profile.

### `raw_duplex`

- capture and render active without a promise of echo suppression;
- UI must warn about feedback risk.

## 6. Processing features

Each feature has three values:

- supported by provider;
- requested by consumer;
- actually enabled for this stream.

Features:

- acoustic echo cancellation (AEC);
- noise suppression (NS);
- automatic gain control (AGC);
- raw/unprocessed capture preference;
- future beam forming and voice isolation.

The provider must not report a feature as enabled merely because the platform API was requested. It reports the actual accepted state when knowable.

## 7. Clock and timestamps

Every stream epoch has one immutable media format and one source clock domain. Each audio frame carries:

- `stream_id`;
- `epoch`;
- monotonically increasing sequence number;
- sender monotonic timestamp;
- first sample index;
- samples per channel;
- flags;
- payload.

Wall-clock time is not used to order audio frames. A new stream epoch is required after a discontinuity that changes sample format, sample index origin or clock domain.

## 8. Receiver behavior

Receiver pipeline:

```text
packet validation
 -> reorder/loss accounting
 -> jitter buffer
 -> decode (if used)
 -> channel/format conversion
 -> dynamic resampling for clock drift
 -> bounded render/capture ring
```

The receiver must track buffer water level. The target may adapt within Profile limits, but the queue remains bounded.

Underflow behavior:

- microphone projection: supply silence and increment underrun counter;
- speaker render: render silence/drop missing frame according to Adapter behavior;
- never replay stale frames from a previous session or epoch.

Overflow behavior:

- discard according to explicit policy, preferably oldest frames for an interactive stream;
- update overrun counter;
- do not allocate an unbounded queue.

## 9. Volume and mute

V1 separates local OS endpoint volume from provider hardware volume.

- Endpoint/session mute is always representable.
- Provider hardware volume control is advertised as an optional capability feature.
- A remote consumer must not assume it can change the phone's global media volume.
- Microphone mute stops or zeros the outgoing media stream and remains visible in UI.

## 10. Errors

Profile-specific error codes should distinguish:

- unsupported format;
- permission denied or revoked;
- audio route unavailable;
- device busy;
- focus denied;
- hardware stream open failure;
- buffer underrun/overrun threshold exceeded;
- clock recovery failure;
- processing feature unavailable;
- transport discontinuity.

## 11. Future compatibility

New optional processing features and formats may be appended in Profile major 1. Changes that reinterpret role, timestamps, channel layout, or permission semantics require Profile major 2.
