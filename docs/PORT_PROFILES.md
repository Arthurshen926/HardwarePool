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

PCM, encoded-media and audio callback rules are retained in
`docs/AUDIO_PROFILE.md` and `docs/DATA_PLANE.md`. `capyio-audio` owns the
direction-neutral selected specification and Session/Route/Stream/epoch media
binding; concrete network framing remains transport-owned. Audio types live in
that crate or Profile-specific descriptors rather than dominating Core.

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
