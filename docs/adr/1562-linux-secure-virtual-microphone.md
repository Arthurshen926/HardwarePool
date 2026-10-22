# ADR 1562: Finite secure mono virtual microphone on Linux

Status: Implementation candidate; mandatory hosted acceptance required.

## Decision

Extend ADR 1561's explicit paired-store secure mono launcher to the existing
`virtual_microphone` endpoint, superseding that slice's Playback-only scope.
Legacy omission still selects UDP. Reverse stereo, virtual speaker, renewal and
unattended service remain outside this decision. No protocol change is required.

The remote network Port remains Source and local network Port remains Sink.
The PipeWire projection is an owned application-selectable Audio/Source, not a
network direction change or a default system microphone selection. Virtual mode
forbids a caller target; its exact node name derives from the original local Sink
port UUID and is independently checked by owner and UI. Catalog export cannot
start a Route, acquire authority or downgrade a secure-only Source.

Existing secure-owner constructors remain selected-target Playback wrappers.
Additive explicit constructors permit only Playback or VirtualMicrophone. Reuse
the existing bounded PipeWire endpoint, created only after fresh bilateral
consent and authenticated media binding. Stop, EOF, peer loss, revocation and
original expiry remove the owned helper/node. Changes require new consent, not
rebind or renewal. Pins, immutable Route/lease/profile binding, original monotonic
consent deadline, rate/queue bounds and no-UDP-fallback remain unchanged.

Route state remains Starting. Actual receive/helper-write counters do not prove
that an application selected or consumed the virtual source. Plain PCM exists at
the authorized local PipeWire boundary. No device permission, physical capture,
implicit keyring setup or physical phone compatibility is claimed.

## Required evidence

Retain all selected-target secure playback and older owner/legacy gates. Add the
same eleven actual PTY controller → fixed Node → isolated protected store → TLS
profiles for virtual microphone. An independent recorder links the exact owned
Audio/Source ports, not a selected-target null-sink monitor shortcut, and measures
ordered tone/silence/tone plus post-cleanup silence. Verify exact process/node/link
ownership and removal without altering unrelated graph objects.

No approval, pretyped/mismatched confirmation, Stop, local/remote EOF, revocation
and exact negotiated expiry remain mandatory. Keep the eight-second expiry
fixture within the unchanged ten-second checkpoint and observe expiry before
cleanup. Local EOF retains honest control-closure/pre-cancellation status evidence,
not fabricated terminal owner telemetry. Pure tests reject target overrides,
wrong Sink-derived identity, unsupported modes and profile/security mismatch.
Compilation/listing is not hosted graph proof; synthetic PCM through the production
Rust sender is not real Android AudioRecord or physical-device acceptance.
