# ADR 1500: Controller single-initiation workflow

Status: cross-node orchestration implemented; physical acceptance in progress.

## Observed mismatch

Controller settings synchronization (ADR 1499) is not resource authorization.
The current receiver offer supplies configuration only. Android source startup
is a separate local action; Windows gamepad projection still has a lab/bootstrap
Route binding. Treating a receiver offer as an instruction to start sensors, or
letting any paired Android peer create a Windows input device, would contradict
the per-resource authorization model. That remote-start shortcut is not implemented.

## Implemented local slice

The explicit Windows controller action now prepares the receiver and creates the
selected virtual device in the host, not by racing two WebView calls. Failure
closes a newly created receiver but preserves an independently started receiver.
Stop attempts both virtual-device cleanup and receiver cleanup. An independently
running DSU workflow is not implicitly stopped by these host commands.
The shared UI moves manual listener controls under advanced diagnostics and
labels the remaining Android start requirement. This is not a complete one-sided
cross-node start, nor a new network/Route interoperability claim.

## Remaining coordinated workflow

1. Select an authenticated peer and DS4, Xbox or motion-only DSU explicitly.
2. Create the input Route with real admitted endpoint IDs, replacing bootstrap
   fixture binding. Request finite per-resource authorization at the owner;
   Windows must also authorize remote-initiated input projection.
3. Bind an operation to the current Session and Route. Prepare the receiver and
   selected projection, then start the authorized Android source. Confirm live
   input before reporting usable; receiver readiness alone is not success.
4. Associate separately authorized haptics with that operation; refusal of
   vibration must not falsely report input failure or grant an actuator.
5. Either endpoint can stop the same operation. Cancel pending starts, clear
   contacts/motors, close only owned workers, and publish completed cleanup.
   Disconnect invalidates the operation; reconnect does not replay a stale start.
6. The same owner state drives both UIs and Android foreground fullscreen entry;
   UI navigation does not own the workers. Existing OS permission/elevation
   requirements remain visible, never silently bypassed.

No dependency, driver, permission or pairing-store change in this slice.
Validation: frontend build and focused host rollback/DSU isolation tests. Physical
single-initiation acceptance remains outstanding; do not present this slice as
completion of the full feature.

## Android consent prerequisite

The owner-side consent gate now accepts the exact local Gamepad Source and
`capyio.input.gamepad-state/1` Profile. Its request carries resourceKind `gamepad`
and the actual Capability ID. Approval creates a finite grant and exclusive
Source lease policy, not a touchpad or actuator grant. Shared UI renders that
distinct scope. Remembered permission remains resource/pair-generation scoped.

Control dispatch distinguishes touchpad, gamepad and haptics explicitly. Until
the admitted gamepad data worker exists, Start stages the Route then reports
`CAPY.GAMEPAD.DATA_WORKER_UNAVAILABLE`/Offline without opening hardware or
acknowledging Active. Desktop's product data-path allow-list is unchanged.

Validation: six owner-consent tests, three shared consent-parser tests, Vue
build and Android ARM64 JNI release build passed. The first JNI link attempt
was blocked executing installed NDK clang in the sandbox; the approved retry
completed. No APK installation or physical single-initiation claim in this slice.

## DS4 admitted-Route projection boundary

`ViiperDs4RouteController::from_authorized_routes` can now construct an owner
from existing control/motion Routes without creating bootstrap Routes or
granting authority. Rebinding validates both endpoints transactionally: exact
local projection Ports, Profiles and backend, same remote peer/Ready Session,
idle Route states and finite unexpired grants. Start rechecks both grants before
advancing either Route, and preserves their expiry instead of replacing it with
the legacy lab's unlimited grant. This constructor opens no device or socket.

The old lab constructor remains for existing callers. Desktop has not yet
switched to this path: the coordinator still needs to negotiate real input
Routes and wire source startup/teardown. Three DS4 fixture tests pass, including
admitted-grant preservation, expiry-before-start and the existing projection
fixture lifecycle. No physical one-sided-start acceptance is claimed.

## Integrated operation (2026-09-29)

The shared controller panel now selects a connected peer and DS4, Xbox or
motion-only DSU. Its start/stop commands reach the authenticated control owner,
not a sequence of independent WebView button calls. The Windows coordinator
uses real published Source and per-mode Sink IDs, finite consent and exclusive
leases. Android-originated proposals require Windows resource confirmation;
Windows-originated proposals use Android's resource-consent gate. An explicit
Android local start grants only the exact echoed operation.

Android's foreground service owns sender startup and teardown. A receiver offer
alone does not start capture. The source reports Active after sending data;
Windows waits for live input and output activation before reporting use. The
combined DS4 projection consumes one admitted AdapterManaged Route containing
controls and optional motion, with no fabricated second IMU permission.
Xbox and DSU also advance that admitted Route, not the old diagnostic fixtures.

The UI follows operation state, opens Android's full-screen touch surface for
active DS4/Xbox, and exits when the operation ends. DSU leaves the touch surface
closed. Separate optional feedback authorization is retained. Diagnostic manual
controls remain under advanced options rather than forming the ordinary workflow.

No new dependency, driver, Android permission, pairing renewal or transport
encryption change is included. Windows-originated local intent is not permanent
authorization for unsolicited future remote input. Either owner may explicitly
remember permission for this peer and exact resource. Windows stores that choice
against the active pairing generation and certificate fingerprint; a new request
still receives a fresh one-hour grant, and pair rotation/revocation invalidates
the saved choice. Android retains its existing remembered resource grants.
App-private controller packets continue over Tailscale.

Validation evidence and physical acceptance must be reported separately. The
combined DS4 fixture verifies one start/activate/stop transition and preserved
finite grant. Desktop compilation, shared UI build and Android composition/JNI
checks do not establish physical end-to-end success on their own.

## Physical integration findings (2026-09-29)

Windows-originated motion-only DSU was started through the shared panel, with
one Android resource confirmation and no separate listener/sender action.
The real DSU subscriber received 120 sequential packets in 1,007 ms with finite,
changing accelerometer/gyroscope values. A single Windows Stop closed the source
and DSU endpoint. This is short-run DSU evidence, not DS4/Xbox acceptance.

The first DS4 run exposed an Android catalog-publication defect: queued controller
status publication selected pure Sinks as Sources when capture changed catalog
availability. The exact publication failure closed the authenticated connection.
Controller publication now uses the same Source-only selection as heartbeat
publication. Shared-resource choices are also persisted as publication intent;
restoring them does not restore grants or start hardware workers.

The UI now exposes the controller operation error rather than hiding a failed
start as idle. New resource requests leave the full-screen controls and bring
the actual confirmation into view. These corrections require physical retesting
before Android-originated startup is marked accepted.

After the publication fix, Windows-originated DS4 opened the Android full-screen
surface automatically and remained active for over two minutes. A Windows HID
reader observed 9,575 reports in five seconds; the RawGameController consumer
observed an Android-injected button change. Windows Stop removed the DS4 device
and closed the Android touch surface. Windows-originated Xbox subsequently
passed the XInput consumer check (518 samples, two packet advances, changed=true).
Android Stop removed that XInput device. This short run does not establish
latency, physical vibration sensation or long-duration stability.

The reverse-start UI required a dedicated, sanitized peer list because Android's
general UI snapshot contains only its local Node. Candidates now come from Ready
Sessions and real published controller Sinks, never from a guessed pairing entry.
Android restart/update retained the explicitly selected Gamepad publication.
The Android-originated request and cancellation were also observed in Windows:
the resource prompt appeared and was removed after cancellation. Approval and
remembered-permission reuse still require a user's Windows confirmation in the
physical acceptance run. No UI automation silently grants Windows input access.
