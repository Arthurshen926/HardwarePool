# ADR 1519: Runtime-driven Linux lab playback worker

Status: implemented; local socket-free checks passed; hosted IPC acceptance pending.
Date: 2026-10-20
Plan: [LINUX-A07](../plans/active/LINUX-A07-runtime-audio-worker.md)

## Decision and scope

Add the platform-owned `RuntimePlayback` library boundary to the existing Linux
PCM crate. It connects an already admitted dynamic `AudioMediaStreamBinding`
and `NodeRuntime` Route to the actual bounded UDP receiver and owned `pw-cat`
endpoint. This is not a second simulated transport. The static lab CLI and the
default inherited-stdio Linux host are unchanged; neither gains a new command,
listener, synthetic authorization or implicit device start.

This is a worker integration slice, **not a complete paired Linux application**.
The Linux host does not yet have an authenticated peer-control admission owner.
Windows/Android pairing, TLS, wire formats and permissions are unchanged. Reuse
`NativeLanRouteBinding` and the shared Runtime lease model rather than inventing
another control protocol or accepting serialized Ready/Authorized assertions.

## Caller and security contract

The caller must authenticate/admit the peer through its control layer, negotiate
and publish compatible AdapterManaged Ports, obtain local consent, authorize a
finite Route, acquire its local Sink lease, prepare the exact selected format
and explicitly begin Start. A Ready Session alone proves none of authentication,
consent or transport integrity. The library cannot manufacture any of these
steps and is not exposed to untrusted JSON or remote commands.

`open_trusted_lab` additionally requires explicit local/expected-peer addresses,
a finite whole-second 1–300 second budget and a specific PipeWire target, with
ADR 1518 address/target policy revalidated even for directly constructed config.
UDP remains unencrypted/unauthenticated; dynamic IDs and live grants do not
prevent spoofing or turn the adapter into secure media. No automatic fallback
to static lab IDs is permitted. This must stay trusted-lab-only until a secure
Android-compatible media transport and host admission composition are delivered.

## Lifecycle

- Before resource creation, require a Linux Runtime, Starting Route, exact live
  Session/Route/epoch binding, remote Source/local Sink, AdapterManaged backend,
  voice-interactive mono 48 kHz S16LE/10 ms preset, selected
  `pcm-s16le-48000-mono` format and Interactive QoS, finite grant and live local
  Sink lease within that grant.
- Open exact-bound UDP then the existing isolated PipeWire process. Revalidate
  after opening before Active. Opening failure moves only the owned Route
  Offline and releases its leases; successful Active means readiness, not sound.
- Each `poll` checks fresh Runtime state and wall time before delivery and again
  after the at-most-10-ms receive. Wall-clock regression, grant change/expiry,
  missing/expired lease, Session closure, catalog invalidation or epoch change
  fails closed. A separate monotonic 1–300 second pump lifetime cannot be extended
  by grant/lease renewal. Existing exact packet/stale/backpressure checks apply.
- Stop closes UDP before bounded child cleanup, then completes only the matching
  Route's stop and releases its leases. Runtime-requested Stopping is consumed
  before more media. Repeated Stop retries cleanup but never resumes data.
  Explicit Stop cleanup failure remains Stopping, retains the lease and does not
  add a Problem; successful retry completes Stop and releases the lease.
  Open/poll failure on the current Starting/Active Route retains a sanitized
  Problem, invalidates its epoch and releases its leases. A stale owner never
  stops or releases a newer epoch's Route. Offline invalidates the epoch, so
  later cleanup retains Offline diagnostics.
- Drop closes resources, but cannot mutate a separately owned Runtime. The host
  must call Stop on owner shutdown and drive `poll` on its worker/event loop.
  There is no background watchdog or autonomous expiry while polling is paused.
  No new PCM is forwarded while paused, but resource closure waits for the next
  poll, explicit Stop or Drop. This distinction is intentional and must remain
  visible in any later host integration.

## Dependencies and compatibility

Reuse only existing workspace Core/Runtime libraries, with testkit dev-only.
No new external package, platform permission, service, driver, module or public
wire change. Production Core/Runtime/Android/Windows implementations are untouched.
The pump now accepts an exact dynamic binding while its existing constructor
retains the static lab behavior.

## Verification

Socket-free tests cover dynamic start/delivery/stop, absent lease preflight,
resource-open failure, expiry during open and receive, changed grant, Session
closure, clock regression, lost lease, stale-epoch isolation, invalid bindings,
finite lifetime and explicit cleanup failure. A required hosted-only test uses
real UDP and an owned `/bin/cat` child with a dynamic binding, then checks bytes
forwarded, child exit, socket rebind and lease release after Stop. This does not
prove peer authentication, actual Android, PipeWire or audible playback.
