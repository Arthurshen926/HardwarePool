# ADR 1502: Explicit stereo native-audio offers

Status: Accepted for the existing trusted-lab native-audio integration

Date: 2026-09-29

## Context

The existing microphone worker uses mono 48 kHz S16LE PCM while the speaker
worker uses stereo. `NativeAudioDataOffer.preset` previously represented only
mono, and the control-owner preparation function required the mono format.
Calling the stereo speaker path with that offer would misdescribe the media.

## Decision

Keep every existing field number, transport value and mono enum value. Append
`NATIVE_AUDIO_PRESET_PCM_48000_S16LE_STEREO_10MS = 2` to the canonical enum.
Expose an explicit preset on the validated offer and prepared worker contract.
Both ends must offer the same preset and select exactly its corresponding
`pcm-s16le-48000-mono` or `pcm-s16le-48000-stereo` format. No implicit channel
conversion, default-to-mono or stereo-to-mono fallback is permitted.

This is an additive format capability within the existing audio Profile and
control protocol major, not a reinterpretation of mono. Existing mono messages
remain byte-compatible. Older builds explicitly reject stereo enum value 2;
coordinated updated peers are required for the stereo Route. Unknown/zero
values remain invalid. Failure is surfaced rather than retried as another PCM
format without user selection.

The authenticated control Session still supplies the peer address. The offer
does not authorize hardware, carry samples or encrypt the existing UDP media.
Finite resource grants, independent microphone/speaker leases, Android system
permission, and ADR 1501's SID-scoped service boundary remain necessary.
No driver installation, service-principal change or media-path replacement is
part of this protocol change.

## Evidence

Protocol tests round-trip mono and stereo, retain their distinct wire values,
and reject malformed endpoint, transport and enum values. Control-link tests
prepare both exact formats and reject mismatched reciprocal presets, changed
formats, missing leases and expired authority. These are hardware-free checks;
they do not establish non-silent physical recording or playback.
