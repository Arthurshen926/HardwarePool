# CAPY-AUDIO-001B — Dedicated Windows virtual speaker

Status: active; B2T and B3 virtual-speaker-to-Android playback and live
endpoint-volume attenuation/mute physically proven

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
2. `CAPY-AUDIO-001B1`: build the pinned, unchanged SysVAD sample on the local
   toolchain and retain WDK/SDK/MSVC/build evidence.
3. `CAPY-AUDIO-001B2`: install the signed test package in an approved target,
   including the ADR 0029 local lab only after recovery preflight, and prove
   endpoint enumeration, playback meter, reboot/restart and clean uninstall.
4. `CAPY-AUDIO-001B2T`: prove how Broker-owned PCM enters the Android transport.
   The current `as-cmd` process captures an endpoint and exposes no supported
   PCM-injection API, so audit a minimal pinned Audio Share server/transport
   slice or reject it before claiming APO-to-phone integration. Use a simulated
   bounded PCM producer before a driver/APO dependency.
5. `CAPY-AUDIO-001B3`: import the minimal reviewed Microsoft endpoint/APO paths
   with MS-PL notices, assign CapyIO-owned identifiers/friendly name, implement
   the bounded APO staging spike and remove unrelated endpoints/features.
6. `CAPY-AUDIO-001B4`: select `CapyIO Speaker` through the existing trusted-host
   picker, prove the APO/Broker bridge reaches Android, and prove the ordinary output remains silent.
7. `CAPY-AUDIO-001B5`: exercise Broker/receiver loss, audio-service restart,
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
- build/install/remove and failure evidence identifies the exact approved
  target and toolchain;
- imported Microsoft paths, notices, license and CapyIO modifications are
  recorded before source enters the repository.

## Current evidence and blocker

- official upstream pinned at
  `717778a20ba4dd2440fe609f69153a1f8a64f597`, MS-PL; the reviewed driver,
  speaker endpoint, SwapAPO and package subset is imported with notices;
- fixed archive SHA-256 is
  `C05B09BB89C929B4E736B54209CD2B4B9B2A382D4D4820F5F9755C0389F7D38A`;
- fixed-revision review rejects SysVAD loopback as real-PCM evidence, rejects
  VirtualDrivers as a functional transport baseline and retains Scream only as
  research evidence; ADR 0028 selects the bounded render APO spike;
- local host: `DESKTOP-AT8EVE9`, x64 Windows build 26200.9168, Visual Studio
  Build Tools 17.14, Windows SDK 10.0.26100.0 and WDK 10.0.26100.6584;
- isolated target `CapyIO-DriverLab` now exists as a Hyper-V Generation 2 VM
  under `F:\CapyIO-DriverLab\vm`, with 8 vCPU, 4–16 GiB dynamic memory, a
  96 GiB dynamic VHDX, Secure Boot and vTPM enabled;
- its Microsoft Windows 11 Enterprise 25H2 Evaluation ZH-CN ISO is 7,371,034,624
  bytes and SHA-256
  `7B4AC87391B659F7724229682B642256289A1C00504056249F0F12029157D3D2`,
  matching Microsoft's published hash list;
- the guest installation reached OOBE but did not complete a stable first boot
  or baseline checkpoint;
- local WDK MSBuild integration, current MSVC Spectre/ATL components and x64
  InfVerif are installed. The minimized x64 Release package builds with signing
  disabled, passes WDK Signability with zero findings and passes independent
  InfVerif `/u` and `/w` validation;
- the refreshed `arthu` login token contains the `Hyper-V Administrators` SID
  and can enumerate/manage the exact lab VM.
- `CAPY-AUDIO-001B2T` is complete at the transport/submission level. The
  CapyIO-authored bounded sender negotiated with the pinned Android v0.3.4 app
  over Tailscale and sent exactly 1,920,000 PCM bytes in 2,000 UDP datagrams
  with no queue-full, missing-receiver or send-error count. Android reported a
  started stereo 48 kHz `AudioTrack`; the human operator clearly heard the
  phone output.

- `CAPY-AUDIO-001B3` now has a compile-validated implementation: the post-mix render APO
  copies real render float32 blocks into a 32-slot/16 KiB fixed shared-memory
  ring without allocation, waiting, I/O, networking or logging in
  `APOProcess`. The Windows Broker owns and validates that mapping, converts
  blocks to S16LE, and feeds the proven bounded Android transport. Windows unit
  tests exercise mapping exclusivity, a committed block, conversion and read
  release.

The package-specific signing and approved installs completed on the exact host
after fresh restore points. A protected global mapping corrected the AudioDG
Session 0 boundary. The MFX placement and initialization fixes now carry real
application playback from the independently selectable CapyIO endpoint to the
Android receiver, including recovery after an Android process/UDP-port change.
No boot-policy or Verifier change has run.

The installed diagnostics proved that Local Service can open and map the global
ring and isolated the original graph-position defect. ADR 0031 moved the bridge
to the composite mode-effect (MFX) position used after mixing by Microsoft's
current componentized SysVAD render sample, and the extension deletes stale SFX
and EFX values before adding MFX PID 14/6. Physical testing then proved audible
Windows-to-phone playback. A later endpoint-volume fix was built, signed and
installed as `oem99.inf` through `oem101.inf`; objective measurement proved its
live notification defect. The direct EndpointVolume callback update is now
installed as `oem102.inf` through `oem104.inf`. One uninterrupted directed
stream measured the expected 100%, 25%, mute and restored-100% amplitudes while
Android remained connected, and the human operator confirmed the corresponding
phone playback changes. The remaining work is B4 lifecycle integration and B5
recovery, upgrade/uninstall and stability checks.
