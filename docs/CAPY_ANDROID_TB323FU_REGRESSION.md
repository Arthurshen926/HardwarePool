# TB323FU integrated Android regression

Device: Lenovo TB323FU, Android 16. Transport: authorized wireless ADB at the
time of the run. Package: `dev.capyio.android` 0.5.11-dev (versionCode 23).

Observed after `adb install -r`:

- the process launched and the app-private Node ID remained
  `<LAB_ID_08>`;
- notification, camera and record-audio grants remained present;
- the shared WebView opened on the unified local-hardware page, not a legacy
  audio-only screen;
- the native catalog exposed seven non-simulated capabilities: microphone,
  speaker, camera, IMU, gamepad, haptics and touchpad;
- microphone, speaker, camera, IMU, gamepad and haptics each accepted a short
  Start, reported `active` without a problem code, accepted Stop and returned
  to `idle`;
- touchpad Start correctly failed closed with
  `CAPY.ANDROID.TOUCHPAD_RECEIVER_UNAVAILABLE` because no Windows receiver/Route
  was active. This is not a touchpad end-to-end pass.

No audio/video recording was retained. This run does not prove media contents,
Windows projection, display frames or cross-device data throughput.

## 2026-09-18 control-plane follow-up

The same TB323FU was upgraded in place over authorized wireless ADB. The final
APK retained the Node ID and all three runtime permissions. Its shared UI again
exposed seven real capabilities; IMU was selected for the final Route gate (an
earlier pass also published microphone, camera, gamepad and touchpad). The
native listener reported its actual ephemeral control port through the Settings
snapshot.

The Windows headless physical gates then proved:

- paired leaf-pinned mTLS, catalog exchange and heartbeat with seven admitted
  capabilities (`CAPYIO_CONTROL_VERIFY_PASSED capabilities=7`);
- an IMU Route proposal over that same connection, independent authorization,
  owner-authoritative exclusive lease, Start, Stop and Release
  (`CAPYIO_ROUTE_VERIFY_PASSED`);
- after disconnect, Android reported zero authenticated connections, zero Routes
  and zero leases.

This follow-up proves the unified control transaction, not IMU sample delivery.
Existing feature-specific transports still require their Adapter endpoints to
be bound to the accepted Route before “connect once, all data flows” can be
claimed.
