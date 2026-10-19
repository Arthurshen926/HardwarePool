# Touch controller 0.5.22 implementation slice

External physical controllers remain outside CapyIO: wired/Bluetooth/USB-IP
input reaches Windows separately. The DSU motion-only option supplements it;
it neither forwards physical controller buttons nor creates a virtual gamepad.

Implemented in the shared Vue surface:

- Fixed, floating-on-down and follow-at-radius joystick centers. Floating down
  begins neutral; each stick retains its own pointer and resets on release,
  cancellation, resize, visibility loss and layout editing.
- Always-visible, touch-visible and invisible stick presentation. Invisible
  controls retain the same bounded touch areas, not the whole screen.
- Mode/display choices persist alongside existing grouped layout preferences;
  old saved layouts remain readable. Defaults are floating and always-visible.
- Explicit L3/R3 buttons, independent of axes, including keyboard activation.
- Fullscreen shoulders and triggers scale with available height; the four
  targets have equal-width columns rather than narrow fixed trigger columns.
- DSU instructions distinguish external button input from phone motion.

## Follow-up in the same undeployed build

Layout editing now separately binds L1/L2/R1/R2, ABXY, both sticks, L3/R3,
Select/Start/Guide, and the D-pad as a unit. Existing four parent-group placements
are preserved. Drag coordinates compensate for parent scaling; horizontal and
vertical sliders permit recovering a moved control without finding it on screen.
Saved coordinates are quantized to keep the existing 2048-byte host preference
limit. Save failure retains editing state rather than falsely claiming success.

Triggers default to press-for-full-travel, with persisted sliding analog mode.
Editing suppresses input. Mode-switch buttons now explicitly stop the active
DSU/virtual controller output before starting the other; a failed stop aborts
the switch and a failed start leaves the old output stopped, not silently restored.

Shared Vue production build and the two stick math checks passed; browser mock
page loaded without console errors. Physical multi-touch/layout and mode-switch
acceptance remains pending as part of the combined validation.

## Unified presentation and opt-in feedback start

Ordinary catalogs now group IMU and haptics Sink under a real gamepad on the
same Node. Standalone sensor-only Nodes retain their entries. The full catalog,
typed Ports and advanced details are unchanged. Android's controller page retains
motion-only start/stop and publication controls and local vibrator strength.

Windows controller settings now select a real remote haptics Sink, request or
stop it and show its Route state in place. Starting DS4/Xbox defaults to requesting
feedback after the projection becomes Active; it does not bypass actuator consent.
Failure is reported separately and does not claim the input projection failed.
Multiple Sinks require explicit selection, and a missing explicit target cannot
silently select another. Feedback ownership is derived from the exact local
game-feedback Source and current Runtime Routes, rather than a transient page
variable. Recreating the WebView recovers the active Sink and stopping/switching
cleans up all Routes owned by that single projection, including pending consent.
Unrelated actuator Sources are not adopted. The control worker independently
cancels pending feedback when the projection/input disappears, while the existing
data worker sends Stop and exits when projection feedback is unavailable.
Failure of remote feedback cleanup does not prevent stopping the local projection.
Process restart does not replay old authorization, recreate an old data epoch or
automatically restart motors; reconnection/start remains an explicit action.

Android now exposes one explicit touch/motion-only/stopped service command.
Touch mode releases independent IMU ownership before starting the touch controller;
motion-only releases touch ownership and feedback before starting IMU. Stop ends
both input owners and feedback. Submitted commands are not labeled successful
capture: the panel displays the actual capability lifecycle. Pending touch frames
are discarded on stop, including when IMU keeps the publisher running. Fullscreen
entry requires an active touch capability and connected receiver, not IMU alone.
The desktop Vue build and Android composition (329 assertions) plus debug APK
build passed. These are not physical mode-switch acceptance results.

Two focused ownership checks cover reconstruction from a serialized Runtime view
and exclusion of unrelated Sources/profiles. Rust desktop check passes, with
three existing unused camera-helper warnings. Physical restart acceptance is
still pending; source-level checks do not prove delivery to the tablet.

The remaining gaps are bidirectional settings synchronization
and combined physical validation. This is not full UX consolidation. No physical
controller forwarding, permission or driver change was added.
