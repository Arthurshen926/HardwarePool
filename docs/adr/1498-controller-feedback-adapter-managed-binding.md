# ADR 1498 — Controller feedback Route integration

Status: implemented for paired Windows-to-Android testing; physical acceptance pending.
The implementation section below supersedes the initial proposal and missing-worker
notes retained as design history.

The controller lab currently has a private UDP input transport, not a generic
authenticated Runtime Route planner. A controller token alone is not actuator
authorization. The generic haptics Route integration remains a separate task.

## Mismatch and affected change

Sending motor bytes back on the current controller UDP socket would be quick,
but would bypass the independent Source-to-Sink Route and Capability grant
required by INPUT_PROFILE.md and SECURITY_MODEL.md. That network change is
paused: no return UDP records, implicit actuator authorization or new permissions
have been added. Receiving output from the local virtual controller and scaling
already-admitted local Sink commands can proceed independently.

## Proposed implementation

1. Publish the Windows projection's typed `capyio.haptics.feedback/1` Source.
2. Select the actual paired Android vibrator Sink from its admitted catalog,
   not a fixture ID. Create an independent Runtime Route with a separate grant.
3. Reuse the existing per-resource confirmation and remembered permission UI.
   The gamepad's input token or input approval must not authorize this Sink.
4. Negotiate an AdapterManaged data binding through the authenticated control
   Session. Bind the data epoch to Route, endpoints, grant expiry and paired
   identity; high-frequency feedback must not travel as JSON-RPC control.
5. Apply short, renewable commands at the Sink and cancel on stop, expiry,
   disconnect or feedback-worker loss. Revoking vibration keeps input working.
6. Exercise real game output in both Xbox and DS4 modes, then revoke, reconnect
   and switch modes. Local vibration alone is not end-to-end acceptance.

Both game motors map to the maximum amplitude of the available single Android
actuator, scaled by the local 0–100% preference. This is not two physical motors.
Existing Android permissions and driver deployment are unchanged.

The proposal preserves the normative model rather than making an exception for
a private feedback token. The implementation and physical acceptance above are
still outstanding. The UI must not describe the existing Windows feedback
counters or the Android strength preference as a working network feedback Route.

## Implemented authorization slice (2026-09-29)

The Android consent gate now handles a local Touchpad Source or a local Haptics
Sink, checking the actual endpoint, direction and Profile. Approval installs the
exclusive lease policy on the owned endpoint (the Sink for haptics), not always
on the Source. Denial, expiry and disconnected Session behavior are unchanged.
Haptics is never dispatched to the touchpad TLS worker.

The host-to-UI request gains `capabilityId` and `resourceKind`; UI accepts the old
touchpad shape for compatibility, but new hosts always provide both scope fields.
Remembered permission binds peer, Capability and pairing generation. Legacy
peer-only entries continue to authorize touchpad only, never the actuator.
The existing JNI names remain for compatibility, not as a transport selection.

The haptics network worker is still absent. A received Start is staged and then
reported Offline with `CAPY.HAPTICS.DATA_WORKER_UNAVAILABLE`; it cannot become
Active from a control acknowledgement alone. The current Android haptics catalog
still advertises its truthful local-only backend. Windows Source publication,
negotiation, data delivery and end-to-end physical acceptance remain pending.

## Implemented data binding (0.5.21, 2026-09-29)

Windows publishes an explicit game-feedback Source when the user requests the
paired Android haptics Sink. The UI requires a running DS4 or Xbox projection
and a live Android controller input peer. Existing paired identities authenticate
an independent TLS 1.3 channel with ALPN `capyio-haptics/1`. There is no new
dependency, permission, driver or input-token-derived actuator authority.

The control RouteStatus carries only a port and StreamId; its IP remains the
authenticated control peer's address. Both sides bind Session, Route, endpoints,
epoch and finite grant expiry. Source and Sink leases are independently checked
and renewed using their actual owned endpoint, including this reverse direction.
The Android catalog now declares AdapterManaged instead of local-only delivery.

The service-owned worker executes a fixed command then acknowledges its sequence.
The Source allows one outstanding command, renews at 80 ms, and limits each effect
to 160 ms. Initial sequence zero must release the actuator before either endpoint
reports Active. Replay, expiry, socket failure, Route stop and loss of forward
controller input terminate the worker; Android cancels in its finally path.
Strength zero cancels and gain is persisted locally. A single actuator uses the
maximum of the two requested game motors, not a claim of dual-motor hardware.

Protocol and TLS tests and both builds passed. Installation and local driver
feedback are not proof of game-to-tablet delivery; physical validation remains
an explicit release condition.
