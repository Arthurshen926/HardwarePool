# Android Host Adapter

The Android host will provide the phone's microphone, speaker and explicitly
shared camera while reusing the shared Rust Core, Protocol and Runtime.

## Camera2 lab

`capyio-camera-app` is now a standalone Camera2/MediaCodec lab. It declares
only `android.permission.CAMERA` and `android.permission.INTERNET`, requests CAMERA after a visible user
action, targets a visible preview plus an AVC encoder Surface and exposes only
bounded frame/access-unit counters. It stops whenever its Activity is not
visible and stores no image. The INTERNET permission is used only by the
foreground exporter. Blank endpoint input preserves the C4 device-loopback ADB
reverse tunnel; C23 also permits one explicit canonical private, link-local or
100.64.0.0/10 Windows IPv4 on fixed port 38173 for a trusted-LAN lab.

One explicitly authorized Android 16 device run accepted the two-Surface session
and produced 1280x720 AVC output. This is not yet the Android Node host, an
production encoded transport, a background service or Windows virtual-camera
roundtrip.
Every future APK install/update still requires an explicitly selected ADB target
and the applicable human authorization.

The Android-free camera contract also encodes the private CAVC v1 config/access-
unit records defined in `docs/AVC_ADAPTER_WIRE.md`. The matched Rust decoder and
validation receiver live in `adapters/vcamdroid`. The trusted-LAN mode has an
exact Windows bind/phone allowlist and no DNS, discovery or wildcard listener,
but remains plaintext and is not production pairing or encryption.

## Native responsibilities

- runtime permission requests;
- foreground-service ownership of active microphone/render sessions;
- persistent privacy notification;
- audio focus and route change handling;
- microphone capture and speaker render Adapter;
- lock-screen/background lifecycle;
- Kotlin ↔ Rust command/event boundary;
- real-device metrics and diagnostics.

## Explicit non-responsibilities

- Android does not register a system-wide virtual microphone or speaker for arbitrary Android apps in the MVP.
- Activity lifetime does not own an active audio session.
- Platform callbacks do not mutate Core state directly; they report completions/events to the Runtime.

## First spike

Create an isolated Audio Lab before networking:

1. enumerate actual input/output route and negotiated format;
2. render a sine/chirp and WAV;
3. capture microphone to WAV;
4. report sample rate, channel count, frames-per-burst and xrun counters;
5. verify foreground-service, lock-screen and permission behavior on the vivo test phone.
