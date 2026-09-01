# ADR 0048: Use VHF as the Precision Touchpad compatibility fallback

Status: accepted

Supersedes the deferred fallback portion of ADR 0041.

## Context

ADR 0041 selected the lower-risk Windows 11 user-mode synthetic touchpad API
before implementing a driver. Physical acceptance now proves that this API can
create an active five-contact Precision Touchpad and can produce tap,
one-finger mouse motion and two-finger pan. Windows accepts both physical and
fixed three-/four-contact input batches, but the configured Shell actions do
not run. The same fixed gestures also fail without Android in the path.

Windows Settings reports that three-finger up/down/left/right are configured
for Task View, Show Desktop and application switching, while four-finger
gestures are configured for notification and virtual-desktop actions. The
failure is therefore not explained by a disabled touchpad or absent user
gesture policy. The 2026 synthetic touchpad API remains documented as
pre-release and does not provide complete physical-compatibility evidence on
this host.

Microsoft's Virtual HID Framework (VHF) can enumerate a HID tree through the
in-box `Vhf.sys` and HID class drivers. A VHF source driver with the mandatory
Windows Precision Touchpad and Configuration collections can therefore test
the physical-HID compatibility path without moving networking, packet parsing,
reconnect or gesture recognition into the kernel.

## Decision

Add a separate `CAPY-PTP-003` compatibility projection with these boundaries:

- a minimal KMDF VHF HID source driver owns only device enumeration, mandatory
  feature reports, a fixed bounded input-report queue and teardown;
- a user-mode Broker validates the existing direction-neutral touchpad frames
  and sends only a versioned, fixed-size local IPC command to the driver;
- the driver never parses CapyIO transport records, JSON, Protobuf or network
  input and never implements Android or Windows gesture policy;
- HID descriptors and report packing have hardware-free byte-exact tests;
- the first slice is compile/static-validation only and imports no third-party
  source;
- installation, signing, service/device creation, reboot and security-policy
  changes remain separate high-risk operations governed by ADR 0029.

The existing user-mode synthetic projection remains available for tap,
one-finger and two-finger operation and as the no-driver fallback.

## Consequences

- Device Manager persistence and Shell three-/four-finger compatibility can be
  tested through the same HID path used by physical Precision Touchpads.
- Kernel code remains small, bounded and independent of network/session logic.
- Driver packaging, signing, recovery and rollback work is unavoidable, so no
  deployment claim follows from a successful compile.
- A VHF build that enumerates is still not Precision Touchpad certification;
  physical gesture acceptance and cleanup evidence remain required.

## References

- Microsoft Virtual HID Framework:
  <https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/virtual-hid-framework--vhf->
- Required Precision Touchpad top-level collections:
  <https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-required-hid-top-level-collections>
- Precision Touchpad collection:
  <https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-windows-precision-touchpad-collection>
