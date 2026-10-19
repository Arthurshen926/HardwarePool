# ADR 1444: Terminal control-channel framing failures

- Status: Accepted
- Date: 2026-09-18

## Context

ADR 1436 bounded individual control records, but a caller could reuse a channel
after a partial read/write, timeout or invalid received frame. Reuse cannot
recover frame alignment and may duplicate a partially delivered command.

## Decision

Both authenticated TLS channel types retain a terminal failed state. Any receive
error or send I/O error marks the channel failed and shuts down its TCP socket.
Later sends/receives return `ChannelFailed` without touching the framing stream.
Invalid local outbound bytes are rejected before I/O and do not fail an otherwise
healthy connection. The first error retains its existing typed diagnostic.

No automatic resend is introduced. Hosts must establish a fresh authenticated
connection, reconcile Runtime Session/Route state and apply the existing epoch
rules rather than replay uncertain commands. This does not itself implement that
application reconnect owner, new Session negotiation or pairing enrollment.

## Acceptance

Tests cover truncated headers/bodies, malformed/empty frames, a timeout after a
written length prefix, no further I/O after failure, corrected local input and
real loopback mTLS failure followed by a fresh authenticated connection. The
loopback test deliberately retains its test binding to isolate transport
behavior; it is not proof of production Session renewal or physical reconnect.

No public wire layout, dependency, Android permission, driver or UI changes.
