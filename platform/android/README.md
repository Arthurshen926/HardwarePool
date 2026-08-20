# Android Host Adapter

The Android host will provide the phone's microphone and speaker while reusing the shared Rust Core, Protocol, Runtime and Vue UI.

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
