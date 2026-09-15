# ADR 1402: Project dual-rumble feedback to the Android vibrator

Status: accepted

Date: 2026-09-16

## Context

The unified Android catalog declared a `capyio.haptics.feedback/1` Sink but left
it unattached. The portable contract carries independent low/high normalized
magnitudes and a bounded duration, while the designated phone exposes one
default vibrator rather than two gamepad motors. Android requires the normal
`VIBRATE` manifest permission.

The user explicitly authorized the remaining Android integration restrictions,
including this permission and a debug APK installation on the designated device.

## Decision

Attach a generation-bound `AndroidHapticsSinkAdapter` owned by the existing Node
service. It projects the larger of the two 0..1000 magnitudes onto the default
vibrator's 1..255 amplitude and preserves the portable 1..10000 ms duration
bound. An explicit Stop cancels vibration. Commands are accepted only for the
currently Active generation; stale, zero-strength, over-range and over-duration
commands fail closed.

The shared Quick Action controls only Adapter lifecycle. Starting it emits one
35 ms low-amplitude confirmation pulse because the action is visible and local;
this is device evidence, not a remote feedback Route. A narrow Binder method is
the future authorized Route ingress and accepts only primitive bounded fields.

## Consequences

- The Android haptics Sink is attached and truthfully reports Available/Busy/
  Offline from hardware presence and lifecycle.
- Dual-motor spatial/frequency fidelity is not claimed on a single actuator.
- No foreground service, listener, endpoint or automatic remote command is added.
- `VIBRATE` is added to the unified manifest; it has no runtime dialog.
- Windows rumble-to-Android delivery still requires authenticated Route/data-plane
  composition.

