# ADR 1421: Own Android connected-device capabilities in the Node service

- Status: accepted for `ANDROID-A01F3`
- Date: 2026-09-17

## Context

The unified Android Activity binds `AudioNodeService` while visible. Audio
already starts that service explicitly and promotes it to a typed foreground
service, but IMU, touch gamepad, touchpad and haptics previously entered their
lifecycles only through the Binder. With no audio capability active, leaving
the Activity removed its final binding and Android destroyed the service. The
A01F2 release policy prevented stuck buttons and contacts before teardown, but
the Activity still indirectly owned these capability lifecycles. That conflicts
with the Node Runtime ownership boundary.

These capabilities exchange bounded control/sensor records with an external
device over the network. Android documents `connectedDevice` as the foreground
service type for interaction with external devices requiring a network
connection. Apps targeting API 34 or later must declare
`FOREGROUND_SERVICE_CONNECTED_DEVICE` and meet one listed runtime prerequisite;
declaring the normal `CHANGE_NETWORK_STATE` permission is the narrow network
prerequisite used here.

## Decision

1. Add `FOREGROUND_SERVICE_CONNECTED_DEVICE` and `CHANGE_NETWORK_STATE` to the
   unified APK. Neither permission has a runtime consent dialog.
2. Add `connectedDevice` to the existing non-exported Node service declaration.
3. Start IMU, gamepad, touchpad and haptics through closed explicit service
   actions. Their Starting, Active and Stopping states hold the connected-device
   foreground type independently of Activity binding and audio state.
   On Android 13+, the visible start flow first requires notification consent;
   denial leaves the capability stopped.
4. Stop removes only the type no longer required. The service stops itself only
   after no audio or connected-device capability owns foreground lifecycle.
5. A foreground-promotion failure closes every affected Adapter, neutralizes
   input state and records `CAPY.ANDROID.FOREGROUND_DENIED` rather than running
   invisibly.
6. The Activity remains an observer/input surface. A01F2 still releases
   UI-owned button/contact state on pause without stopping capabilities.

## Consequences

- Active non-display input/sensor Routes can survive Activity backgrounding
  with a visible notification.
- This change does not grant background camera or microphone access, create a
  Route, authenticate a peer, install a driver, or include Display.
- The notification channel now describes hardware sharing instead of audio
  alone.
- Hardware-free composition tests cover the ownership policy. A real-device
  foreground/background/stop run remains required before claiming physical
  qualification.

## Authorization and evidence

The project owner explicitly authorized the permissions needed to continue the
integration work on 2026-09-17. `platform/android/gradlew.bat capyioCheck
--offline` passes compilation, warnings-as-errors Lint, packaging and 262
composition assertions. Installation of the resulting permission-changing APK
on the current wireless-debug endpoint was not approved by the execution
safety boundary, so this ADR does not claim a device result.
