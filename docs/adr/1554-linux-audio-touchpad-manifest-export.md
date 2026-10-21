# ADR 1554: Explicit audio and touchpad manifest export

Status: experimental; synthetic actual-terminal acceptance required.

## Same snapshot, selection and save boundary

ADR 1553's create-new exporter now supports `prepare-audio` with an explicit
mode (`playback`, `virtual-microphone`, `capture`, `virtual-speaker`) and
`prepare-touchpad`. Camera preparation remains unchanged. All modes use the
same bounded authenticated catalog read, close/reap the Node before selection,
then require separate fresh exact `select REQUEST PEER CAPABILITY PORT` and
`save REQUEST OUTPUT_PATH` decisions. There is no default/first matching Port.
The same owner-only exclusive output, pinned ancestry checks, same-UID/root
cooperation assumption, deadline checks and cancellation limits apply.

## Kind-specific schemas and direction

- Playback and virtual microphone require an Available remote audio Source,
  audio_frames_v1 and exact empty-parameter `pcm-s16le-48000-mono` format.
  The saved `source` is that actual remote Port; local direction is Sink.
- Capture and virtual speaker require an Available remote audio Sink,
  audio_frames_v1 and exact empty-parameter `pcm-s16le-48000-stereo` format.
  The saved `sink` is that actual remote Port; local direction is Source.
- Touchpad requires an Available remote touchpad Source, touchpad_frames_v1 and
  exact empty-parameter `touchpad-frame-v1` format. It retains the existing native
  five-contact, 120 by 70 mm, TLS 1.3 mutual-authentication profile. Export is not
  proof of physical input delivery or an OS gesture.

Each kind also requires the matching Host compatibility hint from the complete
snapshot. Unsupported, direction-swapped, parameterized-only or unavailable Ports
cannot be selected, even if another row is eligible. The existing respective
launcher parser validates the complete generated JSON before presenting Save.
No backend/provider, protocol, media budget or runtime authority policy changes.

## Explicit local settings, not enumeration

Audio always requires explicit listen/media addresses and a 1–300-second finite
ceiling under the existing audio address policy. Physical playback/capture also
requires an explicit local target string; this command neither enumerates nor
verifies local device existence. Virtual microphone/speaker modes forbid a
physical target. The capture manifest uses `selected_source`, not the runtime's
internal `stereo_capture` label. Playback retains `linux_led` proposal mode.
Audio preparation explicitly warns that media remains plaintext unauthenticated
UDP for a trusted isolated lab. Authenticated catalog transport does not change
that limitation. No media security choice is inferred from pairing or export.

Touchpad requires an explicit listen address and the existing 3–60-second ceiling;
its schema has no audio target/media/proposal fields or helper executable override.
Camera's independent 120-frame/about-four-second continuous-video budget remains
unchanged by this increment.

## No Start, persistent approval or authority

Preparation does not enumerate PipeWire, open a device, create a listener/helper,
connect media, or issue a Route, proposal, grant or lease. It never launches the
saved manifest. A snapshot can become stale immediately: each later explicit
playback/capture/touchpad invocation still needs a fresh current peer catalog,
exact runtime bindings, independent local and counterpart approval, and finite
resource authority. There is no renewal, session restart or resource handoff.

## Acceptance

Pure tests cover option/schema and eligibility distinctions. Six additional
actual-PTY groups cover all five endpoint modes and invalid direction/format,
local target/mixed options, selection mismatch, cancellation/EOF and unsafe
output paths. They retain the empty-catalog/EOF and no-resource-command peer
oracle and require the real Node to be reaped before selection. The earlier
16 catalog cases, 64 audio graph cases, 13 camera profiles, touchpad software
proofs and staging/platform gates remain separate mandatory checks. Export tests
only write synthetic nonsecret files in owned temporary fixtures; they do not
launch them or access a real keyring, PipeWire graph, physical device or user file.
