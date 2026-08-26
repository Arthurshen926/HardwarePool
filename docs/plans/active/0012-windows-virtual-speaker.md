# CAPY-AUDIO-001B — Dedicated Windows virtual speaker

Status: active; isolated target provisioned, guest/toolchain installation pending

Owner: Codex

Created: 2026-08-25

Requirements: `FR-SCEN-002`, `FR-ADAPTER-002..005`, `FR-ROUTE-003..005`,
`NFR-RT-001..004`, `NFR-SEC-003`, `NFR-STAB-001..004`

## Objective

Expose an independent Windows render endpoint named `CapyIO Speaker`, bridge
only its real render PCM through a bounded user-mode APO path, and feed the proven Android speaker transport
without playing through the physical or Remote Desktop output.

## Slices

1. `CAPY-AUDIO-001B0`: resolve scope through ADRs 0027/0028, pin SysVAD and
   candidate provenance, audit real-PCM paths, inventory tools and identify an isolated target.
2. `CAPY-AUDIO-001B1`: build the pinned, unchanged SysVAD sample inside the
   isolated target and retain WDK/SDK/MSVC/build evidence.
3. `CAPY-AUDIO-001B2`: install the signed test package in that target, prove
   endpoint enumeration, playback meter, reboot/restart and clean uninstall.
4. `CAPY-AUDIO-001B3`: import the minimal reviewed Microsoft endpoint/APO paths
   with MS-PL notices, assign CapyIO-owned identifiers/friendly name, implement
   the bounded APO staging spike and remove unrelated endpoints/features.
5. `CAPY-AUDIO-001B4`: select `CapyIO Speaker` through the existing trusted-host
   picker, prove the APO/Broker bridge reaches Android, and prove the ordinary output remains silent.
6. `CAPY-AUDIO-001B5`: exercise Broker/receiver loss, audio-service restart,
   endpoint disable/enable, upgrade/uninstall and scoped Driver Verifier.

## Acceptance

- Windows applications enumerate and can explicitly select `CapyIO Speaker`;
- audio routed to it reaches the Android phone and not the current physical/RDP
  endpoint;
- driver absence, phone disconnect and user-mode process exit never hang the
  Windows Audio service;
- the driver contains no network, protocol, codec or UI logic;
- the APO real-time callback performs no blocking, allocation, file I/O,
  network I/O or ordinary logging and has bounded ring-full behavior;
- build/install/remove and failure evidence identifies the exact isolated
  target and toolchain;
- imported Microsoft paths, notices, license and CapyIO modifications are
  recorded before source enters the repository.

## Current evidence and blocker

- official upstream pinned at
  `717778a20ba4dd2440fe609f69153a1f8a64f597`, MS-PL, no source imported;
- fixed archive SHA-256 is
  `C05B09BB89C929B4E736B54209CD2B4B9B2A382D4D4820F5F9755C0389F7D38A`;
- fixed-revision review rejects SysVAD loopback as real-PCM evidence, rejects
  VirtualDrivers as a functional transport baseline and retains Scream only as
  research evidence; ADR 0028 selects the bounded render APO spike;
- daily host: x64 Windows build 26200.8875, Visual Studio Build Tools 17.14,
  Windows SDK 10.0.26100.0;
- isolated target `CapyIO-DriverLab` now exists as a Hyper-V Generation 2 VM
  under `F:\CapyIO-DriverLab\vm`, with 8 vCPU, 4–16 GiB dynamic memory, a
  96 GiB dynamic VHDX, Secure Boot and vTPM enabled;
- its Microsoft Windows 11 Enterprise 25H2 Evaluation ZH-CN ISO is 7,371,034,624
  bytes and SHA-256
  `7B4AC87391B659F7724229682B642256289A1C00504056249F0F12029157D3D2`,
  matching Microsoft's published hash list;
- the guest installation has started, but completed first boot, snapshot and
  WDK evidence have not yet been retained;
- WDK MSBuild integration, InfVerif and the upstream-required v142/ATL/Spectre
  component set are not yet accepted as installed;
- the refreshed `arthu` login token contains the `Hyper-V Administrators` SID
  and can enumerate/manage the exact lab VM.

No driver tools were executed and no driver, signing, guest boot policy or host
security state was changed. The isolated VM itself and its Windows installation
are now the only environment changes. The upstream archive and Microsoft-signed
WDK bootstrapper are verified in ignored local cache only. Progress into the
unchanged SysVAD build requires completed guest setup, a baseline checkpoint
and the compile toolchain; the daily host is not an acceptable driver target.
