# ADR 1505: Display peer-session integration

Status: Manual workflow accepted on the current device; automatic quality update implemented, device acceptance pending.

## Startup automatic quality (2026-09-30)

The shared UI defaults to automatic mode and retains manual presets. Android
selects a landscape, screen-proportional even-pixel mode, at most 2560x1600 and
60 Hz, from its actual display and H.264 decoder capabilities. It tries lower
same-aspect candidates when needed and recommends 5–40 Mbps from pixel rate.
Windows validates the result against bounded adapter policy; native engine
startup remains the actual encoder acceptance step, not a policy support claim.
Both UIs expose final selected values. No network-adaptive bitrate, hot mode
change, new dependency, permission, driver or trust mechanism is introduced.

## Integrated path

Envelope 64 uses the already paired control TLS Session. A fresh operation UUID
correlates negotiation; the authorized Route and epoch correlate launch, ACK,
Ready and presented-frame feedback. Either node can initiate, but each resource
owner retains local consent. Remembered consent is resource-scoped, not blanket
trust. The shared format is `gamestream-h264-sdr-v1`, not generic video support.

The requester-SID service pipe implements Prepare/Commit/Renew. Prepare retains
local engine path/hash and worker policy but accepts typed Route identifiers,
quality and a finite deadline. The peer path neither reads legacy display
credentials nor opens legacy listeners. One 176-byte launch record travels
service -> native node runtime -> paired TLS -> Android native receiver, never
WebView. Its exact 64-byte ACK consumes it. Commit is asynchronous; worker startup
ACK yields Ready, and only presented-frame feedback yields UI Active. Video
remains on the existing GameStream transport. The dedicated launch ALPN helper
described below remains staged and is not used in this product path.

Ongoing locally approved use can extend an unexpired finite authorization.
Core renewal applies only to Starting/Active and strictly extends finite expiry
without changing epoch. It cannot revive stopped, denied, revoked or expired
Routes. Runtime emits RouteChanged; Port leases need separate renewal. Hosts
validate local consent and Session binding before renewing. The service deadline
is generation-scoped. Stop, failure, expiry and disconnect share worker/topology
cleanup. UI Stopping persists until service cleanup finishes.

Both platforms share a node/quality/start/stop panel. Legacy connection files
remain under diagnostics, not a fallback for failed node trust. Quality presets
are requests, not physical encoder guarantees. Both nodes need the new version.

## Earlier implementation seams (retained for compatibility)

The local display service now accepts optional numeric DisplaySessionSettings
on Start through its existing requester-SID pipe. It applies them only to the
in-memory loaded configuration, leaves paths/credentials/identity untouched,
and rejects settings while Starting/Running/Stopping or on Status/Stop. Legacy
Start without settings remains compatible. This is only the service execution
seam: it does not assert peer negotiation, hardware support, or node trust reuse.
Android's fixed expectations still require matching values, so this interface
is not exposed as a standalone UI quality selector yet.

Reuse the proven Windows display worker and Android GameStream receiver. A Node
remains a peer; this Route has a Windows display Source and Android display Sink.
No new dependencies, drivers, or pairing UI are introduced.

The engine host now reuses the internal Apache-2.0 `capyio-control-tls` crate
for a separate `capyio-display-launch/1` ALPN and finite Route binding. This
local dependency shares the existing maintained certificate pinning and lease
checks instead of copying them or adding another TLS library. The alternative
was retaining independently provisioned display credentials, which cannot meet
the requested one-trust workflow. No external package is introduced.
The paired engine-launch exchange checks Session/Route/Stream/epoch and Sink
identity, then requires the exact bounded ACK before producing a launch object.
This adapter boundary must still be composed by the node/service coordinator.

The canonical protobuf capabilities describe actual supported modes and bitrate
bounds. Selection is one exact mutually supported tuple, initially H.264 SDR.
4096 pixels, 120 Hz and 100 Mbps are validation ceilings, not hardware support
claims. Unsupported explicit selections fail without silent quality downgrade.
These messages are staged outside Envelope, not advertised as live support.

Device acceptance checklist:

1. Advertise real Source/Sink modes on the authenticated node connection.
2. Either peer creates a Route bound to peer identity, epoch and finite consent.
3. A requester-scoped service operation accepts typed parameters, not arbitrary
   configuration paths. The current fixed-config Start is insufficient.
4. Send bounded per-session launch material through the authenticated control
   channel. Do not copy node private keys, bypass certificate checks, or send
   video via control. Keep legacy display bundles separate until this is wired.
5. Open the receiver after authorization and sender readiness; report Active
   only after first presented frame.
6. Stop from either end closes the receiver and restores the owned display's
   captured topology. Disconnect and partial startup use the same cleanup.

Protocol/service checks cover negotiation, bounded records, authenticated
delivery, launch/ACK and finite renewal. They do not replace physical acceptance
of both-end start/stop, actual output, renewal and desktop restoration. No fixed
configuration is silently modified by Start.
