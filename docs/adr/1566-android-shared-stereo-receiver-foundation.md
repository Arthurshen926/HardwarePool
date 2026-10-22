# ADR 1566: Android shared stereo receiver and acknowledged PCM foundation

Status: Candidate

## Scope

Add the native foundation needed to receive the explicit 60-second secure stereo
profile inside Android's shared Runtime/control supervisor. This is not the
AudioTrack integration or a new user-facing speaker option. Android catalog
publication, Java playback selection and legacy UDP defaults remain unchanged.
No new permission, foreground-service declaration, pairing credential, persistent
access, renewal, reconnect or Linux Node/console reverse-audio mode is added.

The dedicated stereo descriptor, ALPN, binding and atomic 2128-byte CPYA record
from ADR 1565 remain unchanged. Mono 60/300-second and legacy UDP paths retain
their independent profiles and limits. Unsupported live playback remains an
error, not a reason to run a secure descriptor through DatagramSocket.

## One authoritative Runtime and two-phase control actions

The reusable stereo Sink API owns a Runtime and control link. A live Android
Service instead has one shared authoritative Runtime and a supervised link.
Cloning that Runtime or transferring its link to a media worker would split
lease/revocation authority. The new negotiation state therefore borrows the real
Runtime only for bounded validation and mutation, returning prepared actions.

Control-link prepared lease requests/responses have private immutable bytes,
exact link-instance and TLS-session provenance, and consume-once send ownership.
Preparing a response can acquire the local claim but performs no network write.
The caller releases Runtime/global registry guards before sending, then freshly
revalidates the original generation before committing the result. A failed send
does not roll back into a reusable preparation or automatically reacquire a
lease; the caller terminates/reconciles the Route and releases retained claims.
Closed links reject response preparation before Runtime mutation.

Stereo negotiation permits one initial Source Acquire, one authenticated Sink
Acquire and its exact full local claim, one matching Source response, and one
Start. The claims may cross. Foreign, unsolicited or repeated responses,
reacquisition, renewal, changed claims and replayed Start are rejected. No media
offer is published until both claims and Start are established. Prepared becomes
Starting once; TLS binding and later PCM acceptance do not make the Route Active.

Global control-listener lookup returns a per-listener owner, releasing the
registry before control I/O. This is not a claim that every historical Android
control handler has been refactored to avoid Runtime-held I/O. The new stereo
path separates preparation, send and revalidated commit. Worker shutdown and
finished-handle joining occur outside Runtime and global registries.

## Identity and original time bounds

A media-server identity comes from the exact current paired material that
admitted the accepted or dialed control connection. It is tied to the control
generation and checked against the authenticated peer leaf. A stale server-side
identity cache is not reused merely because the Android endpoint is a media
listener. The listener address is the current authenticated TCP connection's
native local address, not a Java endpoint, wildcard inference or UDP probe.

The original local consent monotonic deadline is retained before negotiation;
Start cannot reconstruct a fresh full lifetime. Admission intersects it with
finite grants and both original lease claims. The selected Route, Session,
endpoints, profile, QoS, epoch and claims remain immutable. Backwards wall-clock
observations fail closed; monotonic time continues to cap authority independently.

The 60-second maximum, 100-record/second rate, four-record burst, 6000-record
work cap and 250 ms operation/record budgets are unchanged. Sending or committing
an offer, waiting for readiness, copying PCM, acknowledging partial progress and
polling status never refresh these bounds. A listener may be prepared outside
Runtime guards, but cannot begin its authenticated transport phase until offer
publication has committed after fresh validation. Bound then permits one local
PCM-enable operation, never an automatic enable.

This slice does not add a slower AudioTrack setup protocol. In particular, the
Source still bounds waiting for an offer after Start to 250 ms. A future Java
lifecycle must fit the existing policy or separately design an explicit bounded
readiness phase; it cannot silently enlarge these timers.

## Copy, write permit and actual acceptance

The native job holds at most one 1920-byte stereo PCM record. A caller supplies
one reusable, exactly sized byte buffer and bounded token metadata. Read exposes
an authorized remaining slice; it does not dequeue the record, advance the
stream, count acceptance or permit another network read.

Each individual platform write uses a process-unique permit bound to the exact
job generation, record offset and remaining length. The caller must use a
nonblocking write and acknowledge only its actual returned byte count. Counts
must be within the remaining slice and aligned to four-byte stereo sample
frames. Negative platform errors must terminate the generation. Zero progress
consumes an attempt without extending the original record deadline.

ACK is consume-once. Duplicate, stale, cross-job, wrong-offset or malformed ACK
terminates admission. A transient busy result means retry that same ACK, not
repeat the platform write. The worker rechecks authoritative original state
before publishing acknowledged byte counters or issuing another permit. A full
record completion alone permits the next network read. Fixed-size buffers and
bounded handoff state do not accumulate a session's PCM in memory.

State/Read/ACK use bounded atomics and try-locks, not Runtime locks, TLS, joins or
ordinary logging. Explicit lifecycle Stop closes admission before independent
socket shutdown. Relevant Runtime changes invalidate admission before their
changed state becomes externally visible. Actual worker exit, not an early
completion flag, gates reaping. A consumed Route remains one-shot after reaping;
late callbacks cannot revive it.

## Future AudioTrack obligations and proof limits

The caller must independently verify actual AudioTrack format, buffer bounds,
routing/lifecycle and release ownership, then use the exact permit/ACK contract.
Copying PCM to Java is neither playback acceptance nor audibility. A successful
ACK reports caller-confirmed aligned acceptance, not rendering or physical
output. Revocation can prevent later permits and reject a late ACK but cannot
retract samples already accepted concurrently by the platform or hardware.

This foundation does not implement those platform obligations, advertise secure
speaker playback, or claim physical Android output. SDK/ARM64 compilation and
synthetic JNI-compatible tests are distinct from Java-to-native execution on a
phone. Independent hosted stereo capture evidence, where executed, proves the
production native negotiation/TLS/read-ACK path with a synthetic acknowledged
sink, not AudioTrack or a complete reverse-audio launcher workflow.

## Validation

Pure tests cover prepared-message ownership/provenance, exact immutable claims
and deadlines, backward clock observations and the record/permit contract.
Mandatory hosted TLS tests cover both control roles, crossed claim ordering,
response/Stop races, offer commit, wrong identity and replay, partial and stale
ACKs, bounded backpressure, original expiry and cleanup. Existing reusable stereo,
mono and legacy suites remain separate and mandatory. Local compile/no-run
results must not be reported as execution of these network or graph tests.
