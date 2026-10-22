# ADR 1560: Explicit Android secure mono microphone binding

## Status

Implementation candidate with required isolated synthetic acceptance following
ADR 1559. No physical phone acceptance is claimed.

## Context and decision

ADR 1559 deliberately stopped at reusable transport owners. This slice connects
the Android microphone Source to that existing pinned TLS 1.3 owner through a
bounded Rust-owned JNI job. It does not yet integrate the Linux Node or console
launcher, and therefore does not claim a complete end-user secure audio flow.

Legacy UDP remains the default and a disjoint implementation. An explicit
`secure_mono_tls13_v1` setting and `microphone-secure` start select only the exact
`pcm-s16le-48000-mono-tls13-v1` format. Eligibility advertisement is not evidence
that an actual microphone opened or that a receiver heard samples. Unknown or
inconsistent transport/schema combinations fail before any legacy socket opens.

The native generation consumes an authenticated offer once, retains the original
grant and Source lease, and owns connection, packetization and writes. The
capture DTO/PCM API exposes no private credentials or data endpoint; existing
pairing/control bootstrap is unchanged. Capture waits for authenticated binding
readiness. Streaming is distinct from readiness and requires a successful media
write; neither state claims receiver audibility. Fixed 960-byte PCM frames enter
a bounded, nonblocking native queue. Sequence, sample index and relative media
timestamps advance only per complete frame, never per capture callback.

Capture requires existing explicit permission and visible service lifecycle.
Actual mono/48 kHz/S16 format and the initial routed device are checked before
publication; a changed route, permission, profile or generation stops that use.
Stop is latched across pending connection registration. The TCP connect preceding
registration is bounded by the original connect deadline, not interruptible by
a shutdown handle that does not exist yet. TLS and later writes are interruptible.

Synthetic platform contracts, hosted Java/SDK/JNI/ARM64 compilation and actual
TLS worker tests are required acceptance evidence. They do not establish physical
phone recording, routing behavior, background capture or microphone compatibility.
No permission declaration, persistent grant, renewal or automatic UDP fallback
is introduced.

## Application lifecycle and bounds

The opt-in is a saved local format preference, not resource consent or permission.
The legacy microphone and speaker remain on their existing separate path. The
secure microphone start uses the closed `microphone-secure` direction and the
selected exact Source/Sink format. Profile edits revoke pending/running native
and Java generations and unpublish before changing the catalog. A stale settings
revision or callback cannot authorize a replacement generation. No fallback,
automatic retry or renegotiation within an old grant is permitted.

Native admission counts both outstanding copies and queued/in-flight PCM against
two fixed slots. JNI validates the opaque runtime/generation and exact byte-array
size before copying. State and PCM admission use nonblocking lookup; the capture
thread never connects or writes a socket. Explicit lifecycle Stop may briefly
synchronize with the independent job registry to retain the exact generation,
then releases the registry before shutdown. It never joins under that lock and
is not a hard nonblocking or arbitrary-scheduler-latency guarantee.

AudioRecord uses the fixed MIC source and system-selected input. It verifies
actual mono/48 kHz/PCM16 and pins the initially routed device before sending PCM.
Later device/routing-generation changes fail rather than silently switching.
These platform metadata observations are not per-sample physical-source
attestation or an atomic association between routing callbacks and recorded PCM.
The platform wrapper requests at most 19,200 bytes of recording storage and uses
a fixed 960-byte direct read scratch plus a 960-byte assembly buffer. These are
application bounds, not a bound on Android driver/AudioFlinger or TLS internals.
Each complete frame has a nonresetting 250 ms/128-attempt assembly budget;
partial and zero reads cannot extend it. The native sender independently retains
the existing 100-packet/s, four-frame burst, finite lifetime and original lease
checks. Neither capture progress nor UI polling renews authority.

Stop revokes admission before platform cleanup. The capture worker alone releases
its recorder after in-flight operations return; callers do not release a recorder
still in use. A process-local reservation prevents overlapping replacement capture
even if a Service is recreated. Failed release is reported as failed cleanup and
quarantines that reservation until process exit, not a clean stop or automatic
retry. Native finished-worker retirement happens outside catalog/control locks;
absent further host polling, one bounded completed handle can remain retained.
Already buffered PCM and arbitrary driver scheduling are not claimed to stop at
an exact physical sample boundary.

## Evidence and remaining integration

The dependency-free fake platform suite exercises partial/invalid reads, actual
format checks, permission and routing loss, pending initialization/read Stop,
queue rejection, stale generation callbacks and cleanup failures. Separate closed
DTO/catalog/UI tests reject mixed transports and rollback failed startup. Hosted
tests run the real Rust job and authenticated control negotiation over pinned TLS,
including binding readiness, revocation, renewal rejection, queue saturation and
reaping. Shared TLS tests separately establish actual write backpressure and Stop.

Existing nine independent PipeWire tone/lifecycle graphs remain reusable-API
evidence, not production Node/console or Java-to-physical-microphone evidence.
The Linux secure playback/virtual-microphone launcher and console must be added
and accepted separately before presenting a complete user workflow. No Windows
secure receiver or encrypted stereo/duplex path is advertised by this slice.
