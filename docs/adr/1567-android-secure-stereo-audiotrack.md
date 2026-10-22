# ADR 1567: Explicit Android secure stereo playback

Status: Candidate

## Scope and compatibility

This increment connects the shared receiver foundation in ADR 1566 to a separate
Android AudioTrack owner and an explicit speaker-only secure selection. The
factory default remains legacy. Microphone legacy/secure 60-second/secure
300-second settings remain independent. The new speaker choice selects only the
existing exact finite stereo TLS descriptor from ADR 1565, capped at 60 seconds.
It does not add a codec, transport, lease renewal, reconnect or longer profile.

The secure owner does not configure or reuse the UDP SpeakerSinkAdapter. Unknown
or inconsistent transport/profile/direction is an error before legacy dispatch.
Catalog publication indicates an eligible exact Sink, not verified hardware
playback. Incoming use still needs fresh local and counterpart consent. Remembered legacy
speaker approval is not reused or created for secure stereo. Changing
the selection retires pending/running use before publishing a replacement; it
does not widen an existing grant. This choice is not an outgoing generic Windows
audio action. A compatible secure Source is required. The Linux capture and
virtual-speaker production launchers remain a separate integration task.

## Immutable native authority

The narrow launch DTO contains identifiers, selection revision, exact profile
and fixed record dimensions, not Java-selected endpoints, credentials or leases.
Current authenticated control identity, Session/Route/Stream/epoch, both original
lease claims, original consent deadline and consume-once offer handling remain
owned by Rust. The native receiver is prepared once for the selected generation.
Java waits for authenticated Bound before constructing its AudioTrack. The
one-shot native PCM enable follows actual play-state and bounded exact-route
readiness, with lifecycle/format/route validation immediately before and after.
Local preparation and polling never renew authority or reset the 250 ms native
record budget. The existing bounded Source offer wait is unchanged.

The receiver Route remains Starting. Copying PCM and constructing/starting an
AudioTrack do not make it Active or establish audible output. Native accepted
counters represent freshly authorized platform-write acknowledgements, not
DAC output, application consumption or speaker audibility.

## Fixed ownership and partial writes

The secure worker owns one reusable 1920-byte PCM buffer and bounded metadata.
Each record contains 480 stereo sample frames, 48 kHz signed 16-bit little-endian
PCM. JNI Read packs the remaining bytes at array index zero. Its metadata offset
is the logical offset within the original record, not an index into that array.
AudioTrack therefore writes from index zero for the returned remaining length;
ACK carries the original token, logical offset and actual accepted byte count.

Only nonblocking writes are used. Positive acceptance must be aligned to the
four-byte stereo sample frame and cannot exceed the returned length. A busy ACK
retries that identical acknowledgement without repeating the platform write.
Zero progress cannot refresh the original record deadline. Invalid counts,
format mismatch, platform exceptions, stale generation or lost authority are
terminal, with no UDP fallback or replay of accepted PCM.

The platform owner validates actual initialized format and bounded buffer
properties rather than treating a requested low-latency mode as granted.
Scheduling, driver behavior and already accepted platform-buffer contents are
not made instantaneous by the native authority checks.

## Lifecycle and cleanup

Playback has its own visible-lifecycle gate; it does not request or inherit
RECORD_AUDIO permission. Polling and stale callbacks cannot reopen a revoked
gate. Stop invalidates native admission before platform cleanup. Platform
initialization, partial writes and acknowledgement contention must all observe
cancellation without opening a replacement generation.

A process-wide reservation prevents overlap until the owned worker has finished
and release is confirmed. Cleanup failure is reported and quarantines ownership
rather than being described as a clean stop. Resource release cannot race a
worker still using the AudioTrack. Pausing/flushing stops owned queued playback
where the platform permits it, but cannot retract samples already rendered.

## Evidence and limitations

Deterministic host Java tests use fake AudioTrack/native boundaries to exercise
buffer offset handling, aligned partial writes, ACK contention, initialization
failure, cancellation, stale generations and cleanup quarantine. Hosted Android
SDK compilation and ARM64/JNI inventory check the production declarations and
platform API use. Existing actual TLS receive-job and independent stereo
PipeWire graphs remain mandatory: those graphs acknowledge a synthetic bounded
Sink, not an Android AudioTrack.

These are distinct proofs. They do not demonstrate a physical phone speaker,
actual AudioTrack driver latency, background/lock-screen playback, Bluetooth
routing, or an end-to-end Linux production-launcher/phone workflow. No device
deployment, permission changes or actual microphone/camera use is part of this
implementation acceptance.
