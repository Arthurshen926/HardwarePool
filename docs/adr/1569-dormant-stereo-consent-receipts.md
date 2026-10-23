# ADR 1569: Dormant exact stereo consent receipt foundation

Status: Candidate

## Scope and release gate

Stage B1 adds a production-capable Rust foundation, not an active handshake
switch. Existing Android JNI and audio dispatch do not call the new gate. The
existing Linux mono playback launcher is not the stereo Source owner and is
unchanged. No Node, console, CLI, permission, service, default, wire format,
credential, renewal, reconnect, dependency or physical-device change is included.

Activation requires a separate coherent bilateral library integration: initial
Pending Draft import, explicit Linux approval sending its original finite
Authorized Draft, fresh Android visible approval, exact bounded Prepared reply,
and the genuine `SecureStereoSourceOwner::new_receiver_assigned_until` handoff
with the same Runtime/control link, original Linux monotonic cap and no Source
lease. Neither this foundation nor its tests establish that integration or a
complete production launcher/phone workflow. An intermediate Android-only
behavioral cutover must not be released.

## Transport provenance is not a DTO

`ValidatedRouteStatus` and `ReceivedControlMessage::RouteStatus` are publicly
constructible. Supplying either with a genuine live link does not prove receipt.
The control decoder therefore records opaque, non-Clone exact stereo Authorized
Draft receipts only after authenticated wire decoding. An opaque control identity
also binds the private connection instance; equal Session/Node IDs and certificate
on a second connection cannot substitute for that instance.

The receipt cache holds at most 16 Route entries per connection. First receipt
wall/monotonic anchors are retained. Exact duplicates do not replace authority or
rearm consumption; conflicting status, bad consumption, decoder failure, close,
drop or overflow invalidate retained authority. Consumed/invalidated entries are
tombstones until link teardown. Overflow disables this dormant facility without
changing generic status/heartbeat admission. A receipt is evidence, not local
consent, a prepared Route, a lease or permission to start media.

## Android dormant state and exact checks

The private gate stages an existing exact Pending Draft and uses the actual
resource-request queue. It retains that queue's original finite request bound,
full Route/Session/endpoint snapshots, explicit speaker selection revision and
current nonzero control generation plus the opaque connection identity. Full
Route equality permits only Pending-to-finite-Authorized peer authorization;
profile, backend, endpoints, epoch, selections, compatible format/QoS sets and
diagnostics cannot change. All six media offer fields must be absent. The local
endpoint must be the explicit Android secure Speaker and the peer the exact Linux
system-audio Source.

Host clones share one gate lineage and tombstones rather than cloning separately
consumable authority. An early allow returns an error without queued approval or
Runtime mutation. A new allow after receipt is required. Denial is available
before receipt. Fresh approval intersects the local 60-second bound with the
original finite peer wall expiry and first-receipt monotonic cap before stamp and
Prepared creation. Sampled instants precede wall samples; observed wall rollback,
monotonic regression, delay, expiry, catalog/selection drift and retired control
fail closed. Neither duplicate handling nor preparation restarts a bound.

Bounded stamps retain the receipt, live control lineage and original generation.
Consumption validates that receipt against the actual supplied control instance
and generation, then the existing exact Runtime/Session/endpoint checks. Generic
legacy stamps are unchanged. Second control binding is rejected and invalidates
the first lineage; this slice deliberately contains no rebind/reconnect policy.

Before local approval an exact duplicate is a no-op. After successful approval an
exact duplicate is also a no-op while the captured Prepared Route is unchanged;
no stamp or expiry is recreated. The future dispatcher must hand post-launch
lifecycle traffic to the negotiation owner rather than treating this prelaunch
gate as a general Route-status handler.

## Partial failure is terminal, not merely Offline or Stopped

The gate consumes shared lineage before any approval mutation. Failure after
finite authorization, stamp capture or preparation must retire the exact Route.
Offline is recoverable and Stopped may be prepared again with retained authority;
neither state alone is revocation.

`NodeRuntime::retire_route_authorization` is a narrow, local-owner operation:
it sets the target to Failed + Revoked, invalidates its epoch, releases only its
leases and cancels only its pending Start operations. The terminal pair cannot
be reauthorized through Runtime; repeated retirement is event-free and idempotent.
Missing/nonlocal targets do not mutate the Runtime. Other Routes, claims and
operations remain unchanged. This is not a generic authorization-policy
relaxation or permission to revoke an unrelated resource. The Android gate
propagates cleanup errors instead of claiming success.

## Validation and proof limits

Real pinned loopback TLS tests cover decoder provenance, fabricated DTO rejection,
identical-binding wrong-instance rejection, replay, original anchors, conflict,
closed/dropped links, malformed input and bounded storage. Android tests build the
actual projected catalog, exchange real catalogs, import Pending through the
existing audio path, list real requests, then exercise the dormant Rust gate.
They cover both control roles, fresh approval, local/peer bound intersection,
clock/delay failures, full Route and offer checks, catalog/profile drift, clones,
request tombstones, duplicate handling, stamp instance/generation checks, and
injected failures after authorize/stamp/prepare. Fault injection is test-only.

These tests prove private production-capable Rust APIs, not activated live JNI,
AudioTrack execution, PipeWire graph integration, audible playback, production
bilateral consent or physical-device behavior. Existing mono/legacy and generic
control admission remain separate regression requirements. Stage B2 activation
and its end-to-end tests remain pending.
