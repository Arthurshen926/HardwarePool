# ADR 0049: Compose the private touchpad transport before platform Sink open

Status: accepted

## Context

The bounded `CPTR` record codec and packet receiver were independently tested,
but the Android ADB lab manually repeated their lifecycle: validate Hello, open
a platform Sink, decode Data, submit, acknowledge and close. A future privileged
Windows Broker needs exactly the same fail-closed ordering. Leaving each I/O host
to reproduce it risks opening VHF before binding validation or acknowledging a
frame before the Sink accepts it.

## Decision

Add `PrivateTouchpadTransportReceiver<F>` to the remote-touchpad Adapter. Its
constructor validates the Route/Stream epoch and complete receiver bounds while
leaving `PrivateTouchpadSinkFactory` unopened. Only an exact full-binding Hello
may invoke the factory. Each Data record must pass outer/embedded transport
validation and the existing packet/sequence/rate receiver before its exact Ack
is returned. Invalid transport data closes an active Sink and permanently faults
the connection; explicit Close, timeout, disconnect and Drop retain bounded
contact release.

The caller must still derive the binding, Stream and descriptor from a current
authorized Runtime Route. This state machine adds no socket, named pipe, peer
authentication, encryption, thread, timer or service policy. The Android ADB lab
uses it only after the existing loopback and explicit desktop-input gates.

## Consequences

- ADB lab and a future least-privilege service Broker can share one ordering and
  acknowledgement boundary.
- Mismatched Hello is proven to leave the platform factory open count at zero.
- The state machine does not make `CPTR` safe on an untrusted network and does
  not decide the Windows service pipe ACL or local-client identity policy.
- Production service hosting must still supply Runtime-owned admission, trusted
  receive clocks, I/O deadlines and authenticated remote transport.
