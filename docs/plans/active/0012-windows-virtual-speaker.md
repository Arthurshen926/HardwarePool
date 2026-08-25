# CAPY-AUDIO-001B — Dedicated Windows virtual speaker

Status: active; isolated target access required before driver execution

Owner: Codex

Created: 2026-08-25

Requirements: `FR-SCEN-002`, `FR-ADAPTER-002..005`, `FR-ROUTE-003..005`,
`NFR-RT-001..004`, `NFR-SEC-003`, `NFR-STAB-001..004`

## Objective

Expose an independent Windows render endpoint named `CapyIO Speaker`, capture
only its audio in user mode, and feed the proven Android speaker transport
without playing through the physical or Remote Desktop output.

## Slices

1. `CAPY-AUDIO-001B0`: resolve scope through ADR 0027, pin SysVAD provenance,
   inventory tools and identify an isolated target.
2. `CAPY-AUDIO-001B1`: build the pinned, unchanged SysVAD sample inside the
   isolated target and retain WDK/SDK/MSVC/build evidence.
3. `CAPY-AUDIO-001B2`: install the signed test package in that target, prove
   endpoint enumeration, playback meter, reboot/restart and clean uninstall.
4. `CAPY-AUDIO-001B3`: import the minimal reviewed SysVAD paths with MS-PL
   notices, assign CapyIO-owned identifiers/friendly name and remove unrelated
   endpoints/features.
5. `CAPY-AUDIO-001B4`: select `CapyIO Speaker` through the existing trusted-host
   picker, prove WASAPI loopback reaches Android, and prove the ordinary output
   remains silent.
6. `CAPY-AUDIO-001B5`: exercise Broker/receiver loss, audio-service restart,
   endpoint disable/enable, upgrade/uninstall and scoped Driver Verifier.

## Acceptance

- Windows applications enumerate and can explicitly select `CapyIO Speaker`;
- audio routed to it reaches the Android phone and not the current physical/RDP
  endpoint;
- driver absence, phone disconnect and user-mode process exit never hang the
  Windows Audio service;
- the driver contains no network, protocol, codec or UI logic;
- build/install/remove and failure evidence identifies the exact isolated
  target and toolchain;
- imported Microsoft paths, notices, license and CapyIO modifications are
  recorded before source enters the repository.

## Current evidence and blocker

- official upstream pinned at
  `717778a20ba4dd2440fe609f69153a1f8a64f597`, MS-PL, no source imported;
- daily host: x64 Windows build 26200.8875, Visual Studio Build Tools 17.14,
  Windows SDK 10.0.26100.0;
- WDK MSBuild integration and InfVerif are not present;
- Hyper-V VM Management is running, but the current account cannot enumerate
  VMs and therefore cannot verify an exact isolated target.

No driver tools were executed and no driver, VM, signing, boot or security state
was changed. Progress beyond `001B0` requires access to one exact isolated VM;
the daily host is not an acceptable substitute.
