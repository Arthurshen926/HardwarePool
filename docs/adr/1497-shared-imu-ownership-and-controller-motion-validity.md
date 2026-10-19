# ADR 1497: Shared IMU ownership and explicit controller motion validity

Status: Accepted by user on 2026-09-28; implementation and physical acceptance in progress

## Observed mismatch

`ControllerPacketJson.encode` changes absent component timestamps from zero to
one. `android_gamepad::decode_packet` only requires positive timestamps before
mapping the packet into an IMU envelope. `ImuSourceAdapter.stop` unregisters the
listener without clearing its cached measurements. Consequently a controller
packet may carry absent or stopped measurements without representing that state.
This conflicts with PORT_PROFILES.md's IMU provenance and timestamp semantics.

`AudioNodeService.beginGamepad` starts touch controls but does not acquire IMU
sampling. The UI's motion-gamepad name therefore implies a composition that the
start operation does not provide. Independent IMU and gamepad currently share
one controller sender and endpoint, not separate independently routed streams.

## Decision

1. Present Touch gamepad with an explicit, persisted Enable motion setting.
   Keep Motion sensors (IMU) as a separately startable resource. Describe the
   current shared destination honestly; do not imply separate Route support.
2. Own the physical sensor registration in the service with two logical users:
   gamepad motion and independent IMU. Register once for the first user; release
   only after the last user stops. Starting the gamepad acquires motion only
   when enabled. Disabling motion must not stop independent IMU use.
3. Separate sampling availability from the gamepad's consent to consume motion.
   Independent IMU activity must not silently re-enable DS4 motion when the
   gamepad motion switch is off.
4. Introduce an explicitly versioned adapter packet contract with optional
   motion (real component timestamps only) and gamepad-motion selection.
   Controls remain usable without sensors. Coordinate updates to Android sender
   and Windows receiver; old peers must not silently misinterpret new semantics.
   Initial rollout requires matched Android/Windows builds: Android emits v2,
   the new Windows decoder also accepts v1 controls but treats missing/sentinel
   or stale motion as unavailable. Old Windows rejects v2 explicitly; there is
   no silent fallback to fabricated motion and no automatic data-version
   negotiation in this slice. The paired deployment constraint is user-visible.
5. On stop, failure or stale samples, invalidate motion and clear cached values.
   Windows must not publish absent motion as a physical IMU sample. A DS4
   projection may neutralize its motion fields for controls-only operation;
   this is projection behavior, not a fabricated sensor measurement. DSU motion
   publication must reflect unavailable input rather than repeat old samples.
6. Show touch-input state and motion availability separately. Do not equate a
   browser button test with physical IMU delivery or application use.

## Acceptance for implementation

- Controls-only start works from a cold launch without prior IMU sampling.
- Enabling motion starts one sensor registration and sends fresh samples.
- Independent IMU and gamepad motion can start/stop in either order without
  stopping the other user or double-registering sensors.
- Turning gamepad motion off while independent IMU remains active disables DS4
  motion but preserves the independent sensor consumer.
- Stop, restart and sensor failure never publish old values as fresh samples.
- Known physical tilt/rotation reaches DS4/DSU, with each path verified rather
  than inferred from UI status. Android/Windows builds are deployed together.

No pairing, driver or platform permission changes are required by this ADR.
Protocol evolution remains scoped to the adapter; no Core dependency or global
Node role is introduced. Separate independent-IMU Routes remain follow-up work.
