# CAPY-GAMEPAD-007A — DS4 motion plus optional XInput compatibility companion

Status: complete for the forward phone touch+IMU sharing path.

## Outcome

- Kept the Runtime-owned `054c:09cc` DS4 controls+IMU projection unchanged.
- Made the complete DS4 the default and added an explicitly opt-in ViGEm Xbox
  360 companion consuming the same normalized controls through the existing
  fixed 20-byte contract.
- Added paired start rollback, safe-neutral stop, source-offline cleanup and
  terminal companion-failure cleanup.
- Added desktop availability/readiness, submitted-packet and rejected-DS4-frame
  telemetry.
- Added a native XInput probe and browser control visualizer. Standard browser
  Gamepad API validation covers controls only and makes no IMU claim.
- Kept the Android lab awake while its foreground controller Activity is
  visible and changed transient WLAN peer deadlines to submit a complete safe
  neutral state without hot-unplugging the virtual controller. Explicit stop
  still offlines the Route and removes the owned device.

## Evidence

- The final 150-second DS4-only physical Gate accepted 7,096 Android packets,
  including 723 non-neutral states, kept the optional XInput companion inactive
  at zero packets and cleaned its exact owned USB/IP port 1.
- Windows Gaming Input uniquely enumerated `054c:09cc`. Its RawGameController
  report advanced 411 timestamps in one observation; a second observation
  proved live phone button, D-pad switch and axis changes with finite axes.
- A separate combined physical Gate previously accepted 14,804 phone packets,
  including 723 non-neutral states, into both DS4 and the opt-in XInput
  companion. The native XInput probe observed 139 packet advances.
- Both the ViGEm and VIIPER Xbox 360 alternatives advanced through native
  XInput on this host, but their Windows Gaming Input timestamp remained zero.
  This host evidence is why CapyIO no longer enables a second Xbox device by
  default for Steam or browser use.
- The sidecar self-test passed.
- A synthetic sidecar stream passed native XInput with 650 samples, 20 packet
  advances and a changed complete state.
- Desktop tests passed with 42 passed and five explicitly ignored physical
  labs; related clippy with warnings denied and Vue TypeScript checking passed.

## Consumer boundary

The complete DS4 is the compatibility target for Raw HID/WGI, Steam and browser
control consumers. DSU remains the explicit motion-only path for Cemu/Dolphin,
and the native DS4 report carries controls plus IMU for DS4-aware consumers.
The standard browser Gamepad API never exposes controller IMU. The included
browser page remains a manual consumer diagnostic; this Gate does not claim a
specific Chromium session passed.

The local build currently copies the reviewed sidecar and managed ViGEm client
beside the debug desktop executable. Product packaging, servicing and driver
availability remain explicit release work. Reverse rumble routing is also a
separate follow-on and is not part of this completed forward-input slice.
