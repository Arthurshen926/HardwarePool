# ADR 0044: Keep initial touchpad packet framing private to the Adapter

Status: accepted

## Context

The direction-neutral `capyio.input.touchpad-frames/1` contract now maps Android
touch surfaces to Windows synthetic Precision Touchpad batches. A remote path
still needs a bounded representation between those platform boundaries.

The repository deliberately keeps continuous touchpad frames out of Protobuf
control envelopes and JSON-RPC sidecar control. `capyio-data-plane` also states
that its semantic envelopes do not select a public wire layout: a transport ADR
must define framing, authentication, replay defense, epoch binding, MTU and
rate policy.

Selecting a production CapyDataPlane transport now would prematurely couple the
touchpad slice to pairing, cryptography, congestion and multi-profile framing.
Using Serde JSON as the live payload would add variable/unbounded parsing to a
high-rate path and would turn a diagnostic representation into a wire promise.

## Decision

Add a private v1 packet codec to `capyio-remote-touchpad-adapter`. It is an
`AdapterManaged` compatibility contract, not the public representation of the
StandardPort Profile.

The codec:

- is constructed for one already-negotiated `InputStreamDescriptor` and
  `TouchpadDescriptor`;
- uses a 32-byte fixed little-endian header and zero to five 24-byte contact
  records;
- has an exact maximum size of 152 bytes;
- repeats epoch, sequence and source timestamp, while StreamId and clock domain
  stay bound to the surrounding Route/session setup;
- preserves button, confidence, contact size and pressure semantics;
- rejects unknown versions, non-exact lengths, reserved bits/bytes,
  non-canonical absent-option fields and contract-invalid snapshots; and
- opens no socket and performs no authentication, authorization, encryption,
  replay-window or reconnect work.

A future transport must authenticate and bind its peer/Route/Stream before
constructing the receiver and advance the codec/receiver epoch explicitly on
reconnect. The Adapter receiver enforces a bounded local fixed-window rate,
strict per-epoch sequence and active-contact idle cleanup; these do not replace
transport admission or cryptographic replay defense. The Windows kernel
boundary never sees this packet; decoded semantic frames enter the user-mode
Sink session.

The follow-on private Route ingress binds a preallocated 1..=64 record queue to
one current Runtime-owned Core Route and expected Sink. It rechecks Active,
authorization/expiry and epoch state at enqueue/pump and fails closed on
overflow or stale residence. This is still Adapter implementation policy and
does not promote the packet to a public transport contract.

## Consequences

- Android-to-Windows semantics can be tested end to end without choosing a
  network stack or injecting desktop input.
- Maximum allocation and packet size are explicit and small.
- The private packet can evolve or be replaced without claiming arbitrary
  StandardPort interoperability.
- A later public CapyDataPlane binding still requires a separate transport ADR,
  security design, fuzz/golden corpus and compatibility commitment.
- The existing generic touch-to-pointer fallback and full touchpad-frame path
  remain distinct behaviors within the Adapter boundary.

## Alternatives considered

- Protobuf/JSON-RPC control: rejected because high-rate input is data, not
  Route/control traffic.
- JSON/Serde live packets: rejected because diagnostic serialization is not the
  wire contract and its size/shape is less constrained.
- Add the binary codec to `capyio-input`: rejected because network framing is
  not direction-neutral input semantics.
- Add the binary codec to `capyio-data-plane`: rejected because that crate is
  transport-independent and explicitly defers wire encoding.
- Start VHF/HID reports now: rejected because user-mode synthetic-touchpad
  submission already succeeds and kernel work does not solve remote framing.
