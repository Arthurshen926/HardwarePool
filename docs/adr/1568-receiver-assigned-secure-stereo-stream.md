# ADR 1568: Receiver-assigned secure stereo StreamId admission

Status: Candidate

## Stage A scope and compatibility

The Android secure stereo receiver independently chooses its StreamId. The
Linux reusable Source previously required that ID before receiving the offer.
Add `SecureStereoReceiverAssignedSourceAdmission` and
`SecureStereoSourceOwner::new_receiver_assigned_until` without changing the
existing strict-ID admission or constructor. The new typed admission contains
no ignored, nil, random placeholder or sentinel StreamId. No wire format,
dependency, capability default, permission or production launcher changes.

## One-shot selection and authority

Only the first authenticated stereo offer receipt on the pinned control link
can select the nonnil ID. Before latching, the Source validates the exact current
Session, peer, full Route, epoch, stereo profile/transport, original finite grant
and both original claims, then consumes the exact receipt. Selection precedes
media TLS, PCM assembly and helper creation. The media address retains the
current authenticated control peer IP and the original certificate pin.

`selected_stream_id()` is diagnostic only: selection may be followed by failed
TLS and does not imply Bound, a helper, Active, PCM acceptance or audibility.
It remains latched after failure or Stop. Duplicate same-ID and changed-ID offers
cannot replace the first receipt. TLS failure, source revocation, control loss,
expiry and Stop remain terminal: no renewal, reacquisition, retries, reconnect,
rebinding or transport fallback. Initial Acquire and Start remain once-only.

Original wall-clock and monotonic consent/grant/lease caps are retained. Waiting
for or processing an offer does not restart the already pending media operation
budget. Only actual TLS Bound can open the selected PipeWire capture helper;
selection alone never does. Existing strict-ID callers still reject a different
receiver ID rather than silently accepting it.

## Validation and evidence limits

Six real-wire tests require independent-ID selection, strict-ID mismatch,
failed media TLS without helper creation, duplicate same/changed-ID rejection,
offer after source claim loss, and a pending handshake retaining its deadline.
Nonblocking control polling must reach actual consumption before assertions;
negative tests identify the intended failure rather than an unrelated timeout.

The original nine exact-ID owner graph cases and two Android receive-job cases
remain unchanged. Separate receiver-assigned cases repeat waveform, Stop, source
and sink revocation, control/media EOF, expiry, backpressure and partial writes,
and actual Android receive-job waveform/partial ACK. CI checks exact listed test
names, executed results and counts of six wire, nine owner and two job cases.
Independent PCM/channel/silence checks and owned cleanup remain required.

These are software fixtures with finite independently supplied owner consents;
their prepared Runtime grants can be shared fixture data. They do not establish
production Android consent negotiation, a complete Linux launcher, physical
AudioTrack execution or audibility. Compilation is not wire or graph execution.

## Stage B remains blocked and separate

Android currently prepares before the finite Linux consent bound is known.
Generic control-link import validation does not supply an exact production
bilateral authority agreement. A future slice needs an exact typed pre-stamp
grant handshake before either Prepared Route is stamped. A wider peer grant
combined with a local timer is not an acceptable workaround. This ADR neither
implements that handshake nor authorizes Node/console production launchers.
