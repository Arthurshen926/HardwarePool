# ADR 1495: Native audio worker preparation and truthful product readiness

Status: accepted for pre-start checks; automatic worker orchestration incomplete

Date: 2026-09-28

## Decision

CatalogControlLink.prepare_native_audio_worker checks a fresh local Runtime
against the expected Route ID and reciprocal endpoint offers. It requires the
link's Ready Session, exact endpoint identities, Route/Session/Stream/epoch,
finite identical grants, exact mono PCM format and matching selected QoS. Both
offers pass the closed ADR 1494 codec checks. The local endpoint must belong to
this Node and have a current Route/Session-scoped lease within the grant. A
caller must explicitly confirm trusted-lab policy for the insecure UDP backend.

The result includes separate grant and local lease deadlines, endpoint ports
and media identities. It is ephemeral preparation data, not a transferable
authorization token. Recheck before worker start/restart, derive peer IP from
the authenticated transport, and supervise revocation, lease loss and peer loss.
This does not attest the remote owner's lease or open any hardware. The owner
of the other endpoint must enforce its own local policy.

No call changes the Route to Active. Media receipt/projection evidence remains
necessary; successful control traffic is not that evidence.

## Product correction

The desktop generic remote-capability path previously accepted a non-touchpad
remote Active reply without owning a local data worker. Block audio Profiles
before Route creation, consent request, lease acquisition or hardware Start
until automatic audio orchestration is connected. Return an explicit message
that unified audio startup is incomplete. Keep the separate audio diagnostic
and compatibility Quick Actions unchanged. Do not block the existing touchpad
path or imply other hardware Profiles are newly validated.

## Remaining work

The Service and desktop loops must still own socket reservation, reciprocal
offer exchange, preparation checks, Android capture and Windows receiver start,
partial-start rollback, lease renewal and media-based readiness. The new
preparation method is not yet invoked by product audio startup. This slice
does not install an APK, overwrite a running desktop, change permissions,
drivers, pairing, dependencies or data transport security.

## Verification

Control-link tests run real loopback mTLS with a hardware-free native-audio
catalog. A live lease and matching offers produce exact preparation values
while Runtime stays Starting. Wrong Route, Stream, epoch, format, endpoint,
grant, absent offer, unconfirmed lab policy, lease loss/expiry and closed link
reject preparation. The control-link suite passes 17 tests and Clippy with
warnings denied. A desktop regression checks audio cannot enter control-only
success and touchpad remains allowed.

Full desktop library regression: 107 passed, 10 physical-device tests ignored,
zero failures. Documentation validation and whitespace checks passed. Ignored
physical tests are not hardware acceptance evidence.
