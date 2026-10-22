# ADR 1563: Explicit finite five-minute secure mono profile

Status: Implementation candidate; mandatory hosted acceptance required.

## Decision

Add a distinct opt-in `pcm-s16le-48000-mono-tls13-300s-v1` descriptor and
`pinned_tls13_mono_300s_v1` launcher selection. Existing secure mono keeps its
60-second ceiling; legacy audio, camera and touchpad policies remain unchanged.
The new profile permits whole-second requested lifetimes up to 300 seconds.
Do not raise the existing global 300-second Port-lease TTL limit. Negotiation
headroom means the actual original Source lease ends earlier than the requested
upper bound. There is no automatic renewal, reconnect, fallback or replacement
of an existing approval, grant, lease, Route generation or profile.

The authenticated catalog and Route must select the exact new descriptor with
empty parameters. Preserve original full lease claims, grants, pins, selected
specification and original monotonic consent deadline through preparation and
all media operations. Unknown, missing or mismatched capability cannot imply
long-profile support. CPYA and TLS framing remain unchanged; the exact immutable
authenticated Route binds the selected profile independently of record bytes.

Only cumulative work scales: at most 30,000 ten-millisecond records, 28,800,000
PCM bytes and 31,920,000 CPYA bytes before TLS overhead. Existing 100-record/s
rate, four-record burst, fixed 960-byte slots, bounded queues and 250 ms operation
stalls do not grow. Counters use checked arithmetic; time and total-work limits
are independent. Existing constructors retain the old 60-second policy.

This slice integrates the Rust Source sender and Linux paired-store playback and
virtual-microphone launcher/console/export paths. Android Java/JNI live
AudioRecord remains explicitly unsupported for the new profile until a separate
capability/UI increment. Do not advertise it on Android or imply phone support.
Starting/written counters remain distinct from consumer delivery or audibility.

## Required evidence

Keep all previous gates. Add a separate long-only hosted harness with actual PTY
console, fixed Node, isolated protected store, production Rust Source sender and
private PipeWire. Independently record late ordered tone windows after 60 seconds
and near the end of a full 300-second virtual-microphone approval. Verify original
negotiated expiry before cleanup, bounded post-expiry silence and owned resource
removal. Also test selected-target Stop after more than 60 seconds. These are
finite five-minute synthetic tests, not hours, physical microphones or phone
compatibility. Long-only checkpoint/output budgets must not relax old short tests.

Pure tests cover old-policy rejection above 60 seconds, explicit 300-second
boundaries, overflow/30,001st-record denial, unchanged burst/queue limits, delayed
preparation, original lease replacement/narrowing, profile mutation, repeated
offers, Stop/revocation/EOF and Java/JNI unsupported-profile denial. Recorder
verification reads bounded windows and caps output size rather than accumulating
unbounded data. No production authority is inferred from test fixtures.

The long graph gate has its own 20-minute CI job and fixed 345-second harness /
360-second process envelopes. Existing jobs and short-test deadlines are not
extended. Requested 300 seconds is the approval ceiling, not a claim that all
300 seconds remain after negotiation.
