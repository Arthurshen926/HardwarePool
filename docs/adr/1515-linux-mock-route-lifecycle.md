# ADR 1515: Explicit Linux mock subprocess Route lifecycle

Status: accepted for implementation, 2026-10-19.

## Context

ADR 1514 provides an empty Linux Node host. The next hardware-free slice needs
real Adapter catalog registration and staged Route control without inventing
Linux audio support or production peer authorization. Runtime Routes currently
require a two-Node Session; there is no session-free local Route API.

## Decision

Add `linux-host --mock-routes` as explicit opt-in. Default operation remains
empty and creates no peer, Session or authorization. The option launches exactly
`capyio-mock-source` and `capyio-mock-sink` beside the current executable, directly,
with no arguments or shell. Manifests are the embedded repository mock manifests.
Both binaries must be built from this repository in that trusted directory. This
is local code execution, not executable authentication or an untrusted plugin API;
matching manifest/Adapter IDs are compatibility checks, not binary verification.
The library accepts a trusted directory for composition, never command strings.

The local Linux Node owns the returned Source catalog; an explicitly named
Synthetic Linux Mock Peer owns the returned Sink catalog. Both children physically
run on the same machine. A synthetic Session supplies the existing Runtime API's
context. This is not discovered, paired or authenticated peer connectivity. A
local identity colliding with the fixed synthetic peer is rejected. Catalog
ownership, identity, direction and `test_only=true` labels are checked before a
candidate Runtime is committed. There is exactly one fixed mock Route, no
unbounded creation API, and no leases. No Core or authentication API is changed.

The existing `capyio.test.samples/1` custom smoke-test Profile, UTF-8 test-token
format and Basic QoS remain intact. `CapyDataPlane` is only this synthetic Route's
backend declaration: it does not mean a data plane is implemented. Start gives
explicit mock-only synthetic consent and stages Runtime Prepare/Starting, then
prepares/starts both children with the Runtime's new epoch. Active requires both
children to return Active with the exact Route ID and epoch. Repeated Start is
idempotent after status revalidation; Stop is idempotent and restart advances the
epoch. No call uses Runtime's immediate-completion demo helper.

Local management adds parameter-free `mock_route_start`, `mock_route_stop` and
`mock_source_crash` within the unchanged version-1 object schema. Without opt-in,
these return `mock_mode_disabled`. Lifecycle failures return
`mock_lifecycle_failed`. The crash command invokes the existing source Sidecar's
test-only exit(23) RPC; it cannot select a PID, path, command or another Adapter.
Status/snapshot and repeated Start recheck child Route identity/state/epoch and
reconcile unexpected exit. This is command-boundary observation, not continuous
background monitoring; idle stored state may be stale until the next observation.

Partial prepare/start/stop failure best-effort stops every prepared participant
using its original child epoch, then drops/reaps both mock children. The faulted
Adapter and the closed counterpart are marked failed with distinct diagnostic
codes. Failed mock composition cannot restart until the host is restarted; local
management remains responsive. Normal shutdown, EOF, oversized input and I/O
errors all release the owned mock pair. Explicit shutdown cleans up before its
acknowledgement. SidecarHost Drop closes stdin, kills a surviving owned child,
waits/reaps it and joins its pipe readers. Normal host cleanup first requests
Route Stop; it does not claim graceful Adapter shutdown completion.

## Evidence and limits

This proves actual child-process catalog/control with simulated Route lifecycle.
There is no Source-to-Sink sample transfer. Each mock independently returns a
finite test sample during Start; the existing SDK ignores that optional extra
field. No audio/video frames, hardware, network listener, files containing trust,
service installation, permissions or real authorization are introduced.

The owner inherits SidecarHost's bounded response wait and diagnostic retention.
Synchronous stdin writes, process waits and pipe-reader joins are not universal
hostile-child deadlines. The known repository mocks do not create descendants.
The host remains sequential inherited-pipe management, not a system daemon.

Tests cover real subprocess lifecycle/restart, strict default opt-out, bounded
snapshot after 256 retained events, command-boundary exit reconciliation, EOF,
oversized request and shutdown. Injected controls cover partial startup rollback,
wrong status Route ID/epoch, Stop failure and dropping both participants. Process
tests have bounded response/exit waits with EOF-first, then kill/reap fallback.
The cloud test environment does not expose spawned descendants through `/proc`;
external descendant PID inventory was not verified. Reaping is implemented by the
existing owned-child supervisor, and fake-control tests verify cleanup dispatch.

Build repository mocks before these process tests; `cargo xtask test` and the
core CI workflow now do so explicitly. No new external production dependency is
added: composition uses existing Core, Runtime, Adapter SDK and Adapter Host;
Testkit is test-only. No full workspace or physical-platform result is implied.
