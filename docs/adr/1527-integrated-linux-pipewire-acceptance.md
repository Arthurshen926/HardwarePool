# ADR 1527: Integrated Android-policy to Linux PipeWire acceptance

- Status: Accepted
- Date: 2026-10-20

## Context

ADR 1526 proves the actual Linux PCM adapter and PipeWire graph, while earlier
command tests prove Android control policy using real authenticated channels.
Separate passing tests do not establish their composition. Basic software
acceptance needs one bounded run crossing both boundaries.

## Decision

Extend the existing private hosted graph harness with an explicit end-to-end
fixture mode. It invokes one ignored integration test by exact name, with
explicit environment and private-marker gates. Ordinary test runs do not execute
this graph/device-style test. The dedicated hosted job requires its success
marker, so an ignored or zero-test result cannot count as acceptance.

The Rust test starts the actual `capyio-node linux-playback --trusted-lab`
subprocess. Existing ephemeral certificates and paired records provide pinned
TLS identity; these are generated test fixtures only, never user credentials or
persistent provisioning. The test uses the real Android catalog schema and
`AndroidCatalogRuntimeV1` coordinator, not a fabricated Ready peer. The catalog
represents synthetic platform permission availability. It does not exercise
Java AudioRecord or Android system permission prompts.

The same invocation must cross authenticated catalog exchange, exact nonce-bound
Linux local approval, independent Android Route consent, finite authorization and
Source lease, reciprocal dynamic endpoint offer, and explicit Start. Synthetic
PCM is encoded/sent using the existing native-audio-lan implementation and the
negotiated binding, through loopback UDP to the production Linux owner. The
owner's real worker invokes fixed `/usr/bin/pw-cat`, which feeds the private
null-sink graph and actual monitor readback. No production code or default
listener behavior is changed.

Before granting local approval, the test pauses at a harness checkpoint. The
harness confirms no target playback node exists and obtains actual silent PCM
from the null-sink monitor. Only then does it release the consent checkpoint.
The same no-playback/new-silent-samples check is repeated after Linux approval
but before independent Android consent. After the full ordered tone/silence/tone
signal, explicit local Stop must end the
owner process and remove its playback node. After a bounded 100 ms graph drain, at least 300 ms of new monitor samples
must be silent; pre-Stop silence cannot satisfy this assertion.
The Android side also stops/releases its synthetic Route lifecycle. These are
bounded negative lifecycle assertions, not a claim of exhaustive attack testing.

The existing owned-group cleanup, isolated configuration and bounded output
limits are retained. Integrated mode has a 45-second operation budget, finite
checkpoint/control waits and an outer 75-second process timeout with cleanup
grace. The hosted job retains its 20-minute cap and existing official distro
package scope. No user server, sound device, microphone, external audio delivery,
SDK license, persistent setting or deployment is involved.

## Acceptance and limitations

A pass requires a single exact-head hosted invocation's explicit
`pinned_tls=true consent=true dynamic_udp=true real_graph=true stop=true cleanup=true`
marker. Build-only, a separately passing control test, or a separately passing
PipeWire graph cannot substitute for it. Pure tests and compilation may run
locally, but local IPC restrictions are not bypassed.

This establishes a software end-to-end trusted-lab path, not physical-device
acceptance. Remaining gates include actual Android microphone/foreground-service
behavior, real paired-device credential setup, real network characteristics and
physical output quality. Android framework/APK assembly is not introduced here.
TLS authenticates control; the lab UDP PCM remains unauthenticated/unencrypted.
No secure-media or production-ready deployment claim follows from this test.
