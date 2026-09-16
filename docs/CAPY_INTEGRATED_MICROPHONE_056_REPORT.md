# Unified 0.5.6 microphone account-isolation acceptance

Date: 2026-09-16

## Result

The unified Android 0.5.6-dev (version code 18) microphone reaches an ordinary
Windows CPAL/WASAPI recording client through the existing native service and
virtual capture endpoint. No driver change was needed. The earlier
`0x80070057` observation is isolated to the restricted execution context in
this comparison; its precise Windows policy cause is not established.

## Controlled comparison

The same existing `cpal_capture_open.exe` was run against the AB13X physical
USB microphone and the CapyIO virtual microphone. Under
`codexsandboxoffline` in Session 1, both enumerated but failed stream creation
with `0x80070057`. Under the actual `arthu` user in Session 1, both opened:

- USB: 99 callbacks, 95,040 samples, RMS 0.00371339, peak 0.05651996;
- CapyIO before Android Start: 99 callbacks, 47,520 samples, zero RMS/peak.

This rules out using that error alone as evidence of a CapyIO-specific driver
regression. It does not claim that every restricted account supports capture.

## Unified source acceptance

The packaged shared Quick Action started the microphone. The existing Windows
service reported `active`, generation 1. A five-second ordinary-user timeline
probe read mono 48 kHz float32 from the CapyIO endpoint. All 50 consecutive
100 ms windows contained nonzero signal. The first held 3,840 samples and the
remaining windows held 4,800 each. Window RMS ranged from 0.00106446 to
0.00197108; maximum peak was 0.00988770.

The Android native bridge reported 1,726,080 processed frames, active generation
1, permission ready, mono 48 kHz PCM16 and no problem code. Its speaker remained
stopped. The same bridge returned version `0.5.6-dev (18)`, source label
`1185e8a466c4aaa77bf01aa02f660355a7e30820-dirty`, build time
`2026-09-16T12:42:09.6570448Z` and a bound Node Service. The APK identity is
recorded in `INTEGRATED_TEST_PACKAGE.md`.

After the shared Stop action returned true, a fresh ordinary-user capture read
99 callbacks / 47,520 samples with exactly zero RMS/peak. The Windows logical
runtime was stopped and confirmed stopped; the temporary CDP forward was
removed. No audio file was created, and no driver or system audio-service
restart was performed.

## Scope

This closes the current short microphone Start/nonzero-capture/Stop gate and
verifies the native diagnostic response on the physical phone. It does not
establish long-duration audio quality, all third-party recording applications,
network recovery, simultaneous capabilities or visual rendering of the
diagnostics panel. Future probes must record execution identity and use a
physical capture endpoint as a control before blaming a virtual driver.
