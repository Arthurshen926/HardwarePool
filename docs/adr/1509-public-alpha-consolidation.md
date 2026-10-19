# ADR 1509: Public Alpha consolidation and current camera contract

Status: Accepted for baseline cleanup and scope freeze on 2026-10-02.
Incremental architecture work and close-window persistence are accepted by the user.
Cross-process host separation is implemented in the continuation recorded below.

## Context and mismatch

The current Runtime installs a Display Sink alongside the Java catalog fixture.
The fixture-only count assertions are stale. Android JNI orchestration fields
are read only on Android but were compiled into host builds. The desktop camera
path no longer invokes its automatic elevated Broker launcher, although the
ADR 1419 paragraph in ARCHITECTURE.md still describes that as current behavior.
The user explicitly selected the current direct projection path as the Alpha
contract, with ordinary-user support retained as an open gap.

## Decision

- Freeze existing capability scope in `release/alpha-scope.json`; mark Gamepad,
  Haptics, Display and IMU Experimental in the shared catalog. This classification
  is a support scope, not proof of reliability across devices.
- Test catalog semantics, ownership and transactional replacement, not fixture
  counts. Keep Android-only orchestration behind the Android target boundary;
  retain host tests for the portable selection policy and catalog.
- Remove only the unreachable automatic camera Broker launch helper. Preserve
  the Broker executable entry, IPC contracts, snapshots and existing adoption
  behavior. The integrated camera Route still needs the currently documented
  elevated desktop environment. Ordinary-user projection is not accepted.
- Keep native-service probing outside the pure compatibility microphone DTO
  projection so hardware-free tests do not adopt an installed service by accident.
- Box the large RouteStatus variants in user-mode control dispatch/deferred
  queues; no wire format, Core model, authorization or queue bound changes.
  This is a Rust API representation change inside the workspace, not a protocol
  version change; workspace consumers are updated together. These are control
  workers, not real-time media callbacks.
- `cargo xtask alpha-check` is the mandatory Windows/Android software gate.
  Missing tools fail; Desktop, frontend and Android are not optional. A pass
  does not authorize publication or substitute for physical acceptance.
- Keep current per-Adapter data planes and finite consent/lease rules. Add no
  production dependencies, new hardware classes or automatic deployment.

## Original migration proposal (historical; implementation updates below)

Follow-up on 2026-10-02: the user requested faster incremental architecture
work without excessive defensive code or tests. The existing Android camera
orchestration is now extracted into AndroidCameraUseModule and used by the
Service poll/Binder/teardown paths, under the same Service monitor. This small
behavior-preserving extraction precedes the physical gates; it does not claim
the larger host/coordinator migration is complete. Java compilation and Lint
validate the extraction; physical behavior remains unverified for this build.

After reliability evidence and release prerequisites close, extract a host-owned
RouteUseCoordinator incrementally, first one existing capability and then a
second. Runtime remains authoritative for state, Session, consent and leases.
The coordinator handles commands/completions, not payload bytes or OS callbacks.
Adapter readiness must distinguish process readiness from observed media.
Every completion must bind Route + Session + epoch; stale completion cannot
activate or stop a replacement. Stop/failure/expiry converge on idempotent cleanup.

Windows headless ownership requires a bounded authenticated local API, worker
shutdown/recovery tests and a real UI-close acceptance run. Android keeps one
existing system Service and its manifest identity; extract internal modules,
not new Services. Renaming the manifest class is not needed for modularity.
Compatibility paths may be removed only after retained physical parity evidence.

## Consequences and validation

The user subsequently deferred physical acceptance because the Android device is
unavailable, selected the current Windows host rather than a VM, and authorized
continued software architecture work. Physical acceptance remains open.

RouteUseCoordinator now owns native audio, camera and display coordinators for
each control connection and dispatches their lifecycle operations, consent,
statuses and shutdown. Existing per-adapter error handling and cleanup order are
preserved. RouteReadiness expresses worker, peer-media and local-frame evidence
for audio/camera without adding a Runtime state machine. A worker-ready flag
alone never satisfies a media requirement. The later window-lifecycle decision and observational metrics are recorded below.

Display command dispatch now executes in the host display module. All DesktopHost
resource fields are private and AppState no longer implements Deref. Commands
explicitly invoke host methods through their Arc handle. This completes the
in-process resource-field encapsulation step, not process isolation: existing
session-scoped static stores, standalone hosting and local IPC remain separate
work. Controller settings do not directly access DesktopHost resource fields.

Fileless enrollment actions, recovery and status projection now execute in the
host enrollment module, with private manager ownership. Resource-consent command
dispatch also uses a host method. Credential, recovery and authorization semantics
are unchanged. Read-only file diagnostics have no host-resource ownership and
remain outside this migration instead of gaining an unnecessary wrapper.

Accepted control listeners, accepted Session attachment, pairing inventory,
credential checks, invitations and discovery-candidate queries now execute in
host connection methods. Controller-use commands execute in the input module.
Listener and gamepad resources are private to DesktopHost. Fileless enrollment,
consent, display and diagnostics still retain some direct field access.

Outbound connection planning, catalog exchange and failure reconciliation now
execute on DesktopHost. Blocking tasks retain the host Arc instead of looking
up Tauri state through AppHandle. Preflight admission and result application use
host methods, preserving admission before asynchronous scheduling. Accepted
connections and remaining diagnostic/pairing field access are not yet migrated.

Synchronous controller/IMU commands now execute in the host input module. IMU
Route, snapshot and worker ownership are private to DesktopHost. Existing
parameters, lock order and lifecycle policy remain unchanged; asynchronous
preflight and connection orchestration still need their remaining field access
migrated before the in-process API boundary can be considered complete.

Sharing selection, demo Route toggles, remote capability use and the four audio
quick-action endpoints now invoke DesktopHost methods. Audio action resources
are private to the host. UUID adaptation remains in Tauri; existing Runtime and
connection checks remain in host orchestration, without duplicating policy.

DesktopHost methods now own snapshot projection, discovery start/stop, peer
disconnect, paired-listener stop, live-IMU stop and demo reset. Corresponding
Tauri commands only adapt arguments and invoke the host. Discovery/audio/gamepad
worker fields are private to the host. Other commands still use transitional
field access; no new IPC, permission or lifecycle policy is introduced.

DesktopHost now owns the production Runtime, control inventory, connections,
platform polling workers and their original teardown sequence. The process
constructs and retains the host before creating Tauri; AppState holds an Arc
handle with transitional in-process field access. Startup does not require a
Tauri Builder, but this is not a standalone executable or an authenticated local
API. No UI-close survival or headless acceptance claim follows from this step.

Native audio and camera share media_lab_endpoints for local egress discovery and
the existing approved IPv4 pair. Typed address comparison replaces duplicate
string comparison without broadening the lab exception or transport security.

RoutePortLeases now shares native audio/camera remote acquisition, local exclusive
acquisition and renewal orchestration. It carries only Route/Port identities;
Runtime remains the lease authority and control_exchange correlates responses.
Acquire accepts Acquired or Renewed; renewal accepts only Renewed, as before.
Existing timestamps, TTL calculation, operation ordering and capability-specific
failure text are preserved. Scheduling and media readiness remain per capability.

Windows native audio and camera coordinators now share RouteStatusInbox for
deferred status handoff. They retain separate inbox instances and the existing
32-key bound, latest-value coalescing, Session-scoped drain and cleanup behavior.
Capability selection and Runtime validation remain at the existing call sites.
This removes duplicate control plumbing without merging capability lifecycle
state machines or claiming the full RouteUseCoordinator migration is complete.

AndroidControlModule now owns saved pairing state, credential loading and the
native control listener lifecycle, including reload and outgoing dial. The
Service retains synchronization and closes the listener before the Runtime.
AndroidFilelessPairing retains PIN enrollment and recovery. Credential wiping,
the single active peer reload constraint and the UI metadata allowlist remain
unchanged. AndroidNodeNotification owns notification presentation only; Service
foreground ownership and manifest declarations remain unchanged.

AndroidCatalogModule now owns publication selection, migration, persistence and
encoding. AndroidSharingSelection still applies the Runtime publication before
committing the selection; Service retains Runtime ownership and foreground
management. Existing preference keys and catalog wire data are unchanged.

AndroidControllerUseModule extracts existing controller Route orchestration and
sanitized UI state. Service-owned gamepad/IMU lifecycle stays behind three local
callbacks, and the legacy receiver-offer path consults module Route ownership.
The existing first-frame timeout and stop order are preserved. This completes
another per-capability extraction, not the proposed cross-capability coordinator.

AndroidResourceConsentModule extracts remembered resource-consent orchestration
from the Service. It retains the existing storage namespace, pairing-generation
matching and ResourceConsentMemory policy. The Service supplies current directory
bytes on each call under its existing monitor; Runtime remains the authority for
resolving a Route request. No additional trust cache or policy layer is introduced.

AndroidAudioUseModule now owns the existing microphone/speaker Route jobs and
their endpoint, readiness and public-state projection. It invokes the existing
Service controller through snapshot/start/stop callbacks under the same monitor.
Route-resource release remains distinct from Service hardware teardown; no new
lifecycle or authorization model is introduced.

The same incremental extraction now covers Android display orchestration in
AndroidDisplayUseModule. Service commands and receiver callbacks retain the
same monitor, while the module owns operation identity, receiver lifecycle and
UI state. Existing quality negotiation, record clearing and presented-frame
readiness are preserved; this is not a shared RouteUseCoordinator migration.

The repository remains an integration Alpha. No claim of full headless reconnect,
production installation, signing or soak stability is added. The five phases and
their unclosed acceptance gates are tracked in `docs/ALPHA_CONSOLIDATION.md`.

## Accepted window lifecycle and media observations (2026-10-02)

The user selected: closing the window keeps the node running; reopening attaches
back to the same node. Windows now destroys the WebView/window on close while
retaining the process-owned DesktopHost. A per-user/session named Mutex arbitrates
startup before Runtime creation. A named Event asks the original process to open
its window; it has no payload and cannot execute a Route command. Explicit menu
exit is distinguished from automatic last-window exit. The event reader is joined
before its handle closes; run_return releases application state before Host Drop.
Recreated windows receive a fresh native exit menu. The local software lifecycle
check passed with the same PID and a normal zero-code exit, with no physical Routes.

This is an incremental in-process Local API and headless window lifecycle, not a
separate UI executable or authenticated cross-process RPC implementation. Tauri's
native event loop remains in the host process. The event uses the creating Windows
account's default object security; no ACL broadening or elevation is added. Mixed
integrity-level reattachment is not promised. Process-restart autodial remains open.

RouteMediaSnapshot carries Session, Route, epoch, observation time, readiness and
available frame evidence for audio, camera and display. Diagnostics request cached
observations through bounded control queues without probing hardware or holding
the connection-map lock while waiting. At most 32 observations per connection and
16 active connections are exposed; exceeding the connection limit returns an error.
Audio samples are activation observations, camera samples follow the existing poll,
and display samples follow Presented messages. Unknown throughput/loss/latency is
not invented. These values do not constitute physical reliability acceptance.

## Independent Windows Host implementation (2026-10-02 continuation)

The user requested completion of the remaining software architecture. The Windows
entry now separates roles by process: `--node-host` creates DesktopHost without a
Tauri application or WebView; normal launch creates only a window Shell. All 53
existing command names and DTO argument shapes enter the same finite dispatcher.
The Shell proxies JSON control requests and never instantiates Runtime or workers.
The same binary supplies both entry modes, so local package contents do not change.

A version-1 local named pipe is scoped to the current Windows SID and login session.
The DACL grants the exact user, remote clients are rejected, and first-instance
creation reserves the endpoint before Runtime startup. The client verifies the
server executable path against its own executable before sending request data.
There is no elevation or permissive low-integrity ACL; the camera permission gap
is retained. Old-version hosts must be exited before using a different install.

Four fixed API workers preserve concurrent reads/control without unbounded thread
creation. Request/response frames are limited to 1 MiB; client nonblocking reads
have a 60-second response deadline, server I/O has 2-second bounds. Response delivery
uses a one-byte acknowledgement rather than an unbounded FlushFileBuffers wait.
Existing platform service endpoints retain their 4 KiB/10-second contracts and
flush behavior. Media payloads never enter the node API. No production dependency
was added; the transport reuses the Windows service crate's named-pipe implementation.

Closing UI exits only the Shell process. Reopening starts another Shell attached to
the same Host. Explicit quit asks the Host to stop, joins its API workers, releases
DesktopHost (existing Route/worker cleanup), then exits the UI. Restart autodial and
physical Route continuity still require their separate acceptance evidence.

## Controller lifecycle consolidation (2026-10-02)

RouteUseCoordinator now also owns the existing controller coordinator and its
GamepadLab handle. Controller begin/stop, consent resolution, status handling,
remote stop, feedback-request flag and disconnect cleanup pass through this owner.
The connection loop retains the original ordering: media tick, feedback cleanup,
controller tick, then feedback request. Status dispatch remains display, camera,
audio, controller, then Runtime fallback. No protocol, lease, permission or
controller data-plane behavior changes. Touchpad/haptics data workers retain their
existing connection-scoped Route and lease lifecycle; this is not a new generic
state machine. Existing controller tests cover behavior; no duplicate suite added.

## Input Route ownership consolidation (2026-10-02 continuation)

InputRoutes now owns the existing touchpad/haptics pending and managed Route maps,
data workers, renewal deadline and controller-feedback association. It is owned by
RouteUseCoordinator, scoped to the same authenticated connection. Capability start
and stop now dispatch through that coordinator without input fallback in the
connection loop. Media tick, feedback cleanup, controller tick and feedback start
retain their order; input polling remains after endpoint advertisement. Existing
60-second pending consent and lease periods are preserved. A renewal link error
still ends the connection; a lease denial stops only its Route. Input workers still
drop on connection exit. No new state machine, retries, protocol or dependency.

## Follow-up correctness fixes (2026-10-03)

Real Route DTOs no longer inherit demo latency/loss/buffer values or fixture
projection descriptions. Unknown media metrics remain null with simulated=false;
only the four explicit demo Routes retain generated metrics. This restores the
existing evidence contract without adding a metrics transport. Native audio startup
errors identify the required Windows consumer/render action without weakening
readiness or changing timeouts.

Runtime-only file upgrades of schema-v3 installations now update the installed
manifest and back it up/restore it with the binaries. Explicit installed-directory
verification accepts the local rollback directory; distribution-package validation
remains strict. APKs, pairing, settings and system components remain outside this
operation.

Input Route Stop now records local Stopping before notifying the peer. This
prevents the data worker from reporting the peer's expected TLS close as a
transport failure during explicit cancellation. Local lease release still runs
when the control exchange fails; unexpected data failures remain visible.
Controller feedback cleanup follows the same ordering before joining its worker.

Concurrent primary startup exposed the old eight-message exchange bound: unrelated
validated status notifications exhausted it before the touchpad reply. Route and
lease exchanges now share a 32-message bound, matching one capability inbox
capacity. Dispatch, correlation, socket timeouts and rejection of unexpected
messages remain unchanged; no command retry is added. The existing TLS boundary
tests cover reply-at-bound acceptance and bounded refusal without draining more.
