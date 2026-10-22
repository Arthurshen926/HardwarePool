# ADR 1564: Explicit Android five-minute secure microphone selection

Status: Implementation candidate; mandatory hosted acceptance required.

## Decision and compatibility

This is the separate Android live-capture/UI increment deferred by ADR 1563.
It supersedes only that ADR's temporary Android advertisement and JNI rejection
boundary, not its finite authority, exact descriptor, rate or work budgets.

Add the closed local selection `secure_mono_tls13_300s_v1`, advertising exactly
`pcm-s16le-48000-mono-tls13-300s-v1`. The factory default remains legacy; the
existing `secure_mono_tls13_v1` selection retains its 60-second maximum. Neither
restoring a preference nor choosing the new maximum creates or extends consent,
a Route, a grant or a lease. Every session still needs the current exact human
approvals. The global 300-second Port-lease limit is unchanged, and negotiation
headroom can shorten the actual original lease. No automatic renewal, restart,
profile fallback or conversion of a running session is allowed.

## Immutable capture and transport boundary

The selected format is authenticated through the catalog, Route, offer and
prepared Rust sender. JNI derives the live job's maximum from that exact profile,
never from a Java-requested duration. It retains the earlier original monotonic
cap, original wall-clock authority and immutable full lease claims. A new profile
must not reset deadlines or permit lease replacement.

Java validates the exact closed secure handoff and pins the selected profile into
the capture generation. The same TLS transport remains distinct from DatagramSocket;
unknown or mismatched formats must fail before either capture or UDP preparation.
Credentials and endpoints remain Rust-owned. Source/profile/revision changes
invalidate pending and active work before publishing the new catalog. Late
permission or recorder callbacks must not restore an invalidated generation.

Record slots, maximum PCM record size, sequence/sample advancement, four-frame
burst allowance, per-operation deadlines and process-wide recorder reservation
remain fixed. Cleanup-failure quarantine is preserved. The 300-second profile's
30,000-record maximum is cumulative work, not an accumulated PCM buffer.
Prepared/ready state does not prove delivery; Streaming still requires an actual
authenticated successful media write and cannot be revived after cancellation.

## Required evidence and limits

Require Java fake-platform contract tests for both secure selections, exact
settings/handoff matching, profile/revision changes, stale permission callbacks,
permission denial, recorder cleanup and no plaintext fallback. Require actual SDK
Java compilation, generated JNI/ARM64 inventory, and production Rust AudioJob/TLS
execution for the long-profile binding and old-profile preservation. Six new
actual-TLS cases make nineteen mandatory production AudioJob cases: both exact
profile caps, pre-connect mismatch, full authority/profile mutation, pending
handshake Stop, cancellation before readiness and original short lease expiry.
Frontend tests reject secure descriptors at the legacy Windows action boundary.
Retain the
full original-expiry virtual graph and late physical Stop evidence from ADR 1563,
as well as all short-profile and legacy gates.

These checks do not operate a physical Android microphone. Synthetic PCM over
actual production TLS/PipeWire proves those software boundaries, not device
AudioRecord behavior, mobile power/background policy or five-minute phone
compatibility. No hours-long or indefinitely renewable session is implemented.
