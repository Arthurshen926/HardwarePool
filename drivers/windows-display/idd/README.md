# CapyIO Windows virtual display driver

This directory contains the isolated UMDF/IddCx adapter. The currently deployed
ABI-v1 slice publishes one CapyIO Phone Display monitor to Windows. ADR 0050
defines the bounded follow-up in which each authorized Display Sink Capability
owns one dynamically arrived monitor. Windows Display Settings owns whether a
monitor is extended or duplicated; the driver does not implement separate
mirror and extend paths.

## Provenance

The initial source is derived from Microsoft's video/IndirectDisplay/IddSampleDriver
at microsoft/Windows-driver-samples revision
717778a20ba4dd2440fe609f69153a1f8a64f597, licensed under MS-PL. See
third_party/THIRD_PARTY.yml, THIRD_PARTY_NOTICES.md, and LICENSES/MS-PL.txt.

CapyIO changes the sample identity, exposes exactly one stable EDID-less
monitor, bounds the advertised modes, fixes the sample's nullable MMCSS handle
cleanup, and copies the newest BGRA swapchain frame into a Broker-owned bounded
mapping. Encoding, authentication and networking remain outside the driver.
The preferred 1980x912 mode exactly matches the approved V2419A phone's 165:76
landscape aspect ratio. Standard 1920x1080, 1280x720 and 1024x768 modes remain
available so Windows can choose a common duplicate topology. User-mode Route
selection must match the committed mode and select an AVC level that covers its
macroblock rate; the driver contains no codec policy.

`CapyIoDisplayControl.h` defines the closed source contract for the dynamic
monitor lifecycle. It is limited to query, bounded inventory, connect and
disconnect metadata, requires a stable 16-byte monitor identity, caps the
adapter at eight monitors, uses read+write-access `METHOD_BUFFERED` IOCTLs and
has exact checked structure sizes. The source-only 0.2.0 candidate enables the
handler, allocates connectors from a fixed array, makes connect/disconnect
idempotent, reports a stable container GUID and selects one v2 mapping per
monitor. The fixed 272-byte inventory lets a restarted user-mode coordinator
reconcile Capability identities without putting reconnect policy in the
driver. Raw pixels continue to use the separate bounded mapping and never
travel through IOCTL. The installed 0.1.1 driver remains unchanged until a
separately approved deployment.

## Build without deployment

From the repository root:

    pwsh -File scripts/build_windows_virtual_display.ps1

The script compiles Release x64 into ignored target/windows-display/idd
directories with signing disabled. It never installs, removes, enables, signs,
or stages a driver.

Driver deployment requires a separately approved isolated Windows target, an
exact package hash, recorded recovery posture, and an exact rollback command.
