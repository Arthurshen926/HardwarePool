# CapyIO Port Profiles

> Status: initial registry; only foundation validation and Mock data exist.

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

PCM and audio callback rules are retained in `docs/AUDIO_PROFILE.md` and
`docs/DATA_PLANE.md`. Audio types live in `capyio-audio` or Profile-specific
descriptors rather than dominating Core.

## Unknown semantics

Unknown optional metadata may be preserved as opaque data. Unknown direction,
required Profile major, enum or required format semantics cause explicit
rejection.

