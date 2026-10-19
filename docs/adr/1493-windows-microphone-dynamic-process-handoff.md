# ADR 1493: Windows microphone dynamic process handoff

Status: accepted for controlled-lab adapter integration only

Date: 2026-09-28

## Decision

The real native virtual microphone helper accepts an explicit private process
argument mode after its local and peer IPv4 endpoints:

```text
--route-v1 <session-uuid> <route-uuid> <stream-uuid> <epoch> <expires-at-ms>
```

This mode is all-or-none. Its bounded parser rejects nil identities, invalid or
Android-incompatible epochs, expired or unsafe timestamps, unknown options and
extra fields before opening the socket or capture ring. Only the exact mono
48 kHz S16LE baseline is supported. Malformed dynamic configuration never falls
back to the legacy fixed laboratory identities. Old positional lab arguments
remain compatible for existing explicitly controlled lab consumers.

NativeMicrophoneSupervisor.start_route revalidates the current Route/Session
against the finite handoff, validates microphone format and passes its dynamic
identities directly to the child process without shell interpretation. A
separate route-v1 readiness prefix prevents accepting the old laboratory
readiness line. As before, readiness means worker setup, not media receipt or
successful ordinary Windows recording-client consumption.

The child captures a monotonic duration budget once and also checks absolute
expiry. It checks before startup completion and before each capture-ring write,
including after blocking receive returns. Grant expiry exits the receive loop
and releases owned resources. The owner must still stop/reap the child when
authorization is revoked, a lease is lost or the Session disconnects.

## Unfinished product boundary

This connects the existing supervisor and actual Windows helper to dynamic
bindings. The node-to-node control offer, Service orchestration, consent/lease
owner and product UI do not yet invoke this new path automatically. Do not
advertise a completed one-click microphone Route. There is no authentication
token in these process arguments and no new network wire field.

Native UDP remains explicitly insecure controlled-lab transport. Dynamic IDs
and control TLS do not authenticate or encrypt its datagrams. No transport
downgrade, driver installation, permission, signing or dependency change occurs.

## Verification

`cargo test --offline -p capyio-native-audio-lan`: 26 tests passed on Windows.
The new coverage includes exact process-argument roundtrip, finite deadlines,
wall-clock rollback, invalid and incomplete values, rejection of stale/stereo
Routes before spawn, dynamic arguments feeding a real loopback UDP receiver,
and an actual helper process rejecting incomplete arguments before hardware
access. Readiness tests reject the legacy prefix in dynamic mode.

`cargo clippy --offline -p capyio-native-audio-lan --all-targets -- -D warnings`
passed. These tests do not establish physical ring writes, current-device audio
or a Windows recording application result. No installed process is replaced.
