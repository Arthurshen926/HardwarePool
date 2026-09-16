# ADR 1410: Select an installed VHF touchpad fallback

Status: accepted

Date: 2026-09-16

## Context

ADR 1409 made the Windows desktop own the bounded Android touchpad receiver,
but deliberately selected only the user-mode synthetic projector. On the
controlled Windows host that projector reached `InjectTouchpadAction` and was
rejected with Win32 error 5. The same host already has the separately reviewed
CapyIO VHF device from the CAPY-PTP-003 gates, and an explicitly authorized
physical run proved that it accepts the unified Android application's complete
contact frames.

The integrated application needs a deterministic way to use that installed
device without turning ordinary application startup into driver deployment or
accepting a user-controlled executable/argument surface.

## Decision

Before launching its fixed touchpad receiver sidecar, the Windows desktop runs
the existing read-only VHF interface probe.

- Exactly one installed CapyIO VHF broker interface selects the fixed `--vhf`
  projection argument.
- No installed interface selects the existing synthetic projection. Multiple
  interfaces or probe failure fail closed through the existing typed action
  failure path.
- The executable path, loopback port, ADB serial selection and all remaining
  arguments stay fixed. UI data cannot request VHF or provide a device path.
- The desktop never installs, updates, enables or removes a driver and never
  changes boot or security policy.
- The installed VHF interface has an administrator-only ACL. A Win32 error 5
  from the VHF sidecar is mapped to
  `CAPY.WINDOWS.TOUCHPAD_ELEVATION_REQUIRED`; the operator must start the one
  CapyIO Windows application as Administrator and retry. The application does
  not auto-elevate or display its own UAC prompt.
- Stop, helper exit and desktop shutdown retain ADR 1409's process and exact
  ADB-reverse cleanup rules.

## Consequences

The controlled composition remains one Android application and one Windows
application. On a host with the installed VHF device, touchpad projection can
produce Windows precision-touchpad reports through the desktop-owned helper;
the Windows application must be elevated for that action. Hosts without VHF
retain the synthetic path and its platform permission behavior.

This is installed-device selection, not deployment. Installer integration,
automatic privilege handoff, production signing and general device discovery
remain outside this decision.
