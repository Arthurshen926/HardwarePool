# ADR 0041: Probe user-mode synthetic Precision Touchpad before VHF

Status: accepted

## Context

The product goal now includes Android touch surfaces and later Windows physical
touchpads driving a remote Windows machine with native multi-finger touchpad
semantics. Mapping contacts to pointer events at the sender cannot preserve the
raw contact lifecycle needed for Windows 3/4-finger policy.

Microsoft documented a 2026 Windows 11 desktop API that creates a
`PT_TOUCHPAD` synthetic pointer device with physical himetric dimensions and up
to five contacts. A generic device is intended to be treated like a physical
touchpad; a gesture-only device excludes mouse motion/click recognition.
`InjectTouchpadAction` additionally represents selected 3/4/5-finger global
actions.

The API documentation is still marked as pre-release. The repository's
installed Windows SDK 10.0.26100.0 does not declare the new creation/action
symbols even though the target operating system's `user32.dll` may export them.
Compile-time linkage is therefore insufficient evidence of availability.

VHF can implement a persistent Precision Touchpad HID device but introduces a
KMDF driver, packaging/signing, install/remove, rollback and larger kernel
review surface. It is not the least-risk first feasibility step.

## Decision

Use a user-mode `SyntheticTouchpadProjection` as the preferred Windows path if
physical acceptance confirms the documented behavior. Resolve these symbols
from the System32 copy of `user32.dll` at runtime:

- `CreateSyntheticPointerDevice2`;
- `InjectSyntheticPointerInput`;
- `InjectTouchpadAction`;
- `DestroySyntheticPointerDevice`.

`CAPY-PTP-000` first implements two bounded probes:

1. a read-only export inventory;
2. an explicit opt-in smoke that creates a five-contact 100 x 60 mm generic
   synthetic touchpad and immediately destroys it without injecting contacts.

The platform crate owns the small audited unsafe FFI boundary. Portable Core
and `capyio-input` remain safe Rust and do not depend on Windows APIs. The
probe loads only System32 `user32.dll`, validates physical size/contact bounds,
and destroys every successfully created handle before unloading the module.

No network, pairing, reconnect, Profile parsing or gesture recognition enters
this native boundary. A later Projection must release all contacts on Route
stop, peer loss, epoch change, Adapter failure and process shutdown before a
fresh epoch can be accepted.

VHF remains the compatibility fallback only after a separate ADR and exact
driver test/deployment plan. A HID mouse or the existing touch-to-pointer
converter may remain a lower-level fallback but is not described as Precision
Touchpad behavior.

## Consequences

- Current Windows builds can be tested even when the installed SDK headers lag
  the runtime exports.
- Export presence and create/destroy success are useful but deliberately weaker
  than gesture acceptance; 1/2/3/4-finger behavior, Settings integration,
  RDP/multi-user behavior and process-loss cleanup remain unproven.
- The primary implementation can remain unprivileged user mode if physical
  acceptance succeeds.
- API drift or incomplete behavior can fall back to VHF without changing the
  direction-neutral touchpad Profile planned by `CAPY-PTP-001`.
- No driver or APK may be installed by this decision.

## References

- Microsoft Precision Touchpad programming guide:
  <https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/precision-touchpad-guide>
- Microsoft `CreateSyntheticPointerDevice2`:
  <https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/createsyntheticpointerdevice2>
- Microsoft synthetic device parameters:
  <https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-synthetic_device_creation_params>
- Microsoft `InjectTouchpadAction`:
  <https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/injecttouchpadaction>
- Microsoft Virtual HID Framework:
  <https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/virtual-hid-framework--vhf->
