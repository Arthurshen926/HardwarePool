# ADR 0027: Make a dedicated Windows virtual speaker the Gate 7B target

Status: accepted; data-path decision superseded by ADR 0028

ADR 0028 retains this ADR's dedicated-endpoint and isolated-target decisions,
but replaces the WASAPI-loopback data path after fixed-revision source review
showed that SysVAD loopback generates a synthetic tone rather than the real
render stream.

## Context

The Audio Share Gate 7A path proved that Windows system audio can reach the
authorized Android speaker, but it captures an existing physical or Remote
Desktop render endpoint. The product owner requires an independent Windows
device named `CapyIO Speaker`, so applications can route selected audio to the
phone without also targeting the current local speaker.

The former Roadmap described a dedicated virtual render endpoint as a Gate 7
non-goal. That conflicts with the clarified product target and must be resolved
before driver work begins.

Microsoft documents SysVAD as its WDM virtual-audio sample and a starting point
for custom audio drivers. The sample implements virtual render devices with
WaveRT and exposes loopback pins. The upstream Windows-driver-samples repository
is MS-PL licensed and requires driver packages to be signed before Windows will
load them.

## Decision

Split the work into two evidence tracks:

- Gate 7A retains the already proven Audio Share transport and mirror-mode
  lifecycle evidence.
- Gate 7B adds a minimal SysVAD-derived Windows render endpoint named `CapyIO
  Speaker` and makes that endpoint the source selected by the 7A transport.

The original first data path used Windows Audio Engine rendering plus standard
user-mode WASAPI loopback capture. ADR 0028 supersedes that part of this
decision with a bounded render-APO-to-Broker bridge. No network, pairing,
codec, JSON/Protobuf or reconnect logic enters the driver or APO callback.

All driver build, install, restart, uninstall, Verifier and signing experiments
run only in an identified Hyper-V Generation 2 VM or dedicated installation.
The daily-development host is never a driver target.

## Consequences

- The desktop endpoint picker implemented in `CAPY-AUDIO-001A4` becomes the
  immediate integration seam for selecting `CapyIO Speaker`.
- Gate 7 is not complete merely because mirror mode is audible.
- SysVAD provenance and MS-PL obligations must be recorded before source import.
- A working unsigned/development driver is not a distributable feature;
  production signing, installer, upgrade/removal and recovery remain separate
  acceptance work.
- SysVAD loopback is synthetic and cannot prove real PCM transport. ADR 0028
  selects an APO bridge first and retains bounded driver IPC as a fallback.

## Sources

- Microsoft SysVAD sample:
  <https://github.com/microsoft/Windows-driver-samples/tree/main/audio/sysvad>
- Microsoft audio driver guidance:
  <https://learn.microsoft.com/windows-hardware/drivers/audio/audio-universal-drivers>
- Windows Driver Samples MS-PL:
  <https://github.com/microsoft/Windows-driver-samples/blob/main/LICENSE>
