# CapyIO Port Profiles

> Status: initial registry; IMU v1 has deterministic fixture/replay semantics
> and an authorized SensorServer physical-lab path. Production transport remains
> unimplemented.

## Identity and versioning

Profile IDs use `<reverse-style-name>/<major>`, for example
`capyio.audio.frames/1`. A major version changes semantic interpretation. New
optional formats or metadata can be appended within a major only when old
consumers remain correct.

Initial reserved names:

```text
capyio.audio.frames/1
capyio.video.frames/1
capyio.display.frames/1
capyio.input.key-events/1
capyio.input.pointer-events/1
capyio.input.touch-events/1
capyio.input.touchpad-frames/1
capyio.input.gamepad-state/1
capyio.motion.imu-samples/1
capyio.location.fixes/1
capyio.haptics.feedback/1
capyio.control.commands/1
capyio.data.bytes/1
```

## Compatibility

A direct Route requires:

1. Source and Sink directions;
2. equal Profile name and major;
3. at least one mutually supported format descriptor when formats are declared;
4. at least one mutually supported QoS mode when QoS is declared;
5. a compatible interoperability mode.

Different Profiles require an explicit Converter Adapter with one Sink and one
Source Port. Profile names are never inferred from display names.

## QoS vocabulary

- `Basic`: stability and compatibility first.
- `Interactive`: latency and prompt feedback first.
- `Measurement`: timestamps, raw provenance and traceability first.

Profiles may define more specific descriptors without changing these intents.

## Timing metadata

Profiles that represent sampled or measured data can require:

```text
source_timestamp
receive_timestamp
clock_domain_id
sequence_number
units
coordinate_frame
accuracy
calibration
format
```

Unix wall time alone is insufficient for multi-sensor synchronization.

## Audio specialization

PCM and audio callback rules are retained in `docs/AUDIO_PROFILE.md` and
`docs/DATA_PLANE.md`. Audio types live in `capyio-audio` or Profile-specific
descriptors rather than dominating Core.

## Video specialization

Decoded packed-raw video, exact candidate selection, frame metadata and camera
capability semantics are defined in `docs/VIDEO_PROFILE.md` and
`capyio-video`. Encoded H.264/H.265/RTSP paths are not StandardPort v1 by
implication; a vertical Adapter keeps its private data plane `AdapterManaged`
until a complete codec/access-unit contract and Converter are explicit.

## Input and haptics specialization

Pointer, generic touch snapshot, physical touchpad frames, semantic keyboard,
fixed gamepad state, haptics and shared epoch/sequence behavior are defined in
`docs/INPUT_PROFILE.md` and `capyio-input`. Generic touch and touchpad frames
are distinct Profiles and are never silently reinterpreted. Canonical names
include `capyio.input.touchpad-frames/1`, `capyio.input.gamepad-state/1` and
`capyio.haptics.feedback/1`; the earlier mock-only `gamepad` and
`haptics.pattern` spellings are invalid.

ADR 0044's remote-touchpad packet preserves the touchpad frame semantics but is
a private AdapterManaged framing. Its existence does not make that byte layout
the interoperable StandardPort wire representation.

The accompanying private receiver adds bounded sequence, fixed-window rate,
local-arrival-clock and active-idle lifecycle enforcement for one already-bound
Route/Sink. These Adapter policies do not change the StandardPort Profile or
establish peer trust.

The fixed private ingress further binds those policies to one current
AdapterManaged Core Route and expected Sink Port. Route authorization/epoch
checks and queue bounds are lifecycle constraints, not additional Profile
semantics or StandardPort interoperability.

## IMU samples v1

`capyio.motion.imu-samples/1` uses a StandardPort `DataEnvelope` with:

- typed `StreamId`, positive stream epoch and monotonic sequence;
- source and receiver timestamps plus a named clock domain;
- optional acceleration, angular-velocity and magnetic-field component source
  timestamps for Adapters that combine asynchronous sensors;
- acceleration in metres per second squared;
- angular velocity in radians per second;
- optional magnetic field in microtesla;
- Android device coordinates (X right, Y up, Z out of the screen);
- explicit accuracy and calibration state;
- bounded sensor name/vendor/version/type metadata.

Unknown major versions or required enum semantics are rejected. Gaps, stale
epochs, duplicates, late samples and per-consumer overflow are observable and
are not repaired by changing timestamps. The committed fixture uses
`android.sensor.elapsed_realtime` only as a semantic clock-domain label; it was
not captured from the connected phone.

Adding optional component timestamps is an append-only v1 semantic extension.
Older fixtures omit the field and retain their existing meaning. When present,
required component timestamps are positive; a magnetic-field timestamp and
value appear together. The combined envelope source timestamp is the maximum of
the included components, while each original timestamp remains available.

The first SensorServer mapping uses the Android `SensorEvent` elapsed-realtime
clock, accepts documented accuracy values 0–3, preserves the device coordinate
frame and marks calibration `Raw` because the external service does not declare
a calibration state. Pairing within a configured skew is not sensor fusion and
does not imply synchronized sampling.

## Unknown semantics

Unknown optional metadata may be preserved as opaque data. Unknown direction,
required Profile major, enum or required format semantics cause explicit
rejection.
