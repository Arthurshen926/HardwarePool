# ADR 1503: Route-owned controlled-lab camera handoff

Status: Accepted; Windows/Android source integration compiled, physical verification pending

Date: 2026-09-29

## Decision

Reuse the existing Camera2 / MediaCodec CAVC sender, Windows AVC decoder and
software-camera projection. Do not replace codecs, install drivers, broaden the
old camera broker pipe or introduce arbitrary launch paths. The existing C23
trusted-lab network boundary remains fixed TCP 38173 with an exact admitted
IPv4 peer. Product orchestration is limited to the explicitly approved Windows
<LAB_IPV4_04> and Y700 <LAB_IPV4_01> pair; this is not production media security.

An authenticated control Session carries a typed CameraDataOffer from the Sink
with a nonzero Stream UUID and fixed port. Its enclosing finite-authorized
AdapterManaged video Route supplies epoch and selected cavc-avc-access-unit-v1
format. Pairing alone never starts camera capture: a resource proposal requires
the normal local consent or a current per-resource remembered approval, and
both endpoint Port leases. Android CAMERA permission remains separately required.

The background control owner starts the fixed Windows receiver before sending
the offer and Start. The receiver's closed launch arguments optionally carry
the expected Stream UUID, Route epoch and finite expiry as one complete tuple.
It rejects a Config with different identities before decoder creation. A child
watchdog enforces both monotonic remaining budget and wall-clock grant expiry
even while waiting for a TCP connection or blocked on a partial record. This
does not turn plaintext CAVC into authenticated media; an allowed peer remains
inside the trusted lab boundary.

Android uses the admitted stream/epoch rather than generating an unrelated
identity. Windows remains Starting until its current receiver reports actual
published decoded frames. A low-frequency bounded progress line is control
telemetry, not video payload. Stop, revocation, timeout or Session loss stops
only the owning receiver and camera projection and releases the Route leases.
The compatibility camera action cannot take over or stop a Route-owned session.

## Limits and verification

One Windows software-camera producer is exclusive; audio remains independently
owned. No new external dependencies, driver deployment, APK installation,
permission change, publication or commit is implied by source implementation.
Compile checks and identity/deadline/offer tests are prerequisites, not physical
camera evidence. Paired deployment, visible Windows camera frames and clean
stop/restart must be verified before claiming the one-click flow works.

The Windows coordinator uses the existing resource-use command and consent queue.
The Android JNI/Java sender binding has compiled with fixed Stream/epoch/expiry;
its first sent access unit reports Source activity, not Windows rendering. The
Android Service checks grant/lease/Session liveness at its existing bounded tick;
the sender also holds a monotonic finite deadline. No new permission is added.
The
Windows receiver must be deployed together with the desktop because its new
closed binding arguments are intentionally not understood by older binaries.
