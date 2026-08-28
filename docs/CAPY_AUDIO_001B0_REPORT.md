# CAPY-AUDIO-001B0 Report

Date: 2026-08-26

Status: architecture/candidate audit complete; local compile baseline proven;
controlled deployment preflight pending

## Outcome

The Windows speaker target is now explicitly an independent system render
device named `CapyIO Speaker`, not permanent mirror mode. ADR 0027 separates
the proven Audio Share transport into Gate 7A and the driver-backed projection
into Gate 7B. ADR 0028 corrects the original data-path assumption after source
review showed that SysVAD loopback produces a synthetic tone.

The chosen first architecture is:

```text
Windows application
  -> CapyIO Speaker (minimal SysVAD-derived WaveRT endpoint)
  -> endpoint-associated render APO
  -> bounded shared-memory/SPSC staging ring
  -> user-mode Broker
  -> Audio Share-compatible user-mode transport ingest (B2T decision pending)
  -> Android speaker
```

This avoids adding networking or a custom PCM ring to the kernel driver. The
existing trusted endpoint picker is reusable once `CapyIO Speaker` enumerates.

The APO callback is limited to a preallocated, non-blocking copy/drop path. The
exact shared-memory setup is still a lab spike, not a proven ABI. The current
external `as-cmd` process captures a Windows endpoint and has no supported
Broker PCM-injection interface, so B2T must resolve that ingest contract before
the architecture can claim APO-to-phone delivery.

## Candidate audit

- SysVAD is retained for toolchain and endpoint-enumeration evidence, but its
  synthetic loopback cannot validate real PCM.
- VirtualDrivers/Virtual-Audio-Driver exposes endpoints but discards render PCM
  by default (or writes a debug file) and returns capture silence. It has no
  supported user-mode PCM boundary; the reviewed signed release also omits the
  notices file present on current main.
- Scream demonstrates a real render tap and bounded staging, but its WSK and
  IVSHMEM kernel transports violate CapyIO's thin-driver boundary.

No candidate source or binary was imported. Exact revisions and hashes are in
`third_party/THIRD_PARTY.yml`.

## Provenance and local inventory

- Microsoft Windows-driver-samples pinned at
  `717778a20ba4dd2440fe609f69153a1f8a64f597`;
- upstream path `audio/sysvad`, license MS-PL;
- official fixed-revision archive SHA-256
  `C05B09BB89C929B4E736B54209CD2B4B9B2A382D4D4820F5F9755C0389F7D38A`;
- extracted upstream `LICENSE` SHA-256
  `07639618C7B94AB9953CC61715F6325877E1854DCA611ED9B1BD54497CF5E93A`;
- no upstream source or binary imported;
- identified local host: `DESKTOP-AT8EVE9`, AMD64 Windows build 26200.9168;
- Visual Studio Build Tools 17.14, Windows SDK 10.0.26100.0 and WDK
  10.0.26100.6584 present;
- WDK MSBuild integration and x64 InfVerif execute locally;
- the pinned WIL submodule revision is
  `3c00e7f1d8cf9930bbb8e5be3ef0df65c84e8928`; its ignored-cache archive
  SHA-256 is
  `0E4974FF5F74B2AFBE95E8F85DDC6B7329ADF5718A228D54E4B21DA132A1A13E`;
- existing v142 ATL plus WIL produced x64 Release `SwapAPO.dll` with SHA-256
  `87447A0C5796093561E4B1DC4A531969A19E8B7FED3D82E2CA74BDADE0E8347C`;
- the full solution still requires current-toolset ATL/Spectre, a deliberate
  signing-disabled compile configuration and reconciliation of the upstream
  base INF with current InfVerif `/w` rules. The APO and extension INFs pass;
- `DESKTOP-AT8EVE9\arthu` is a member of the built-in `Hyper-V Administrators`
  group, and the refreshed login token contains group SID `S-1-5-32-578`;
- `CapyIO-DriverLab` is an identified Hyper-V Generation 2 target at
  `F:\CapyIO-DriverLab\vm`, configured with 8 vCPU, 4–16 GiB dynamic memory,
  a 96 GiB dynamic VHDX, Secure Boot and vTPM;
- Microsoft Windows 11 Enterprise 25H2 Evaluation ZH-CN was downloaded from
  the Evaluation Center. Its 7,371,034,624-byte ISO SHA-256 is
  `7B4AC87391B659F7724229682B642256289A1C00504056249F0F12029157D3D2`,
  matching Microsoft's published `Enterprise Eval x64 Eval ZH-CN DVD9`
  value;
- Windows guest installation reached OOBE but did not produce a stable first
  boot, baseline checkpoint or usable recovery target.

The first direct GitHub query timed out without a proxy; the read-only pinned
revision query succeeded through the user-provided Clash proxy at
`127.0.0.1:7897`.

## Safety and validation

WDK compile and InfVerif tools ran locally. No driver install/remove, signing
command, BitLocker/test-signing/Verifier operation or repository source import
occurred. VM creation, Secure Boot/vTPM configuration and guest OS installation
remain confined to the identified `CapyIO-DriverLab` target.
Repository validation now requires the SysVAD and candidate revision/license
records, the `CapyIO Speaker`/APO/bounded/isolated-target boundary, and rejects
driver source while the provenance record remains `source_imported: false`.

The source archive was downloaded and inspected only under the ignored
`.agent-cache` directory. The Microsoft-signed WDK 26100.6584 bootstrapper was
cached, verified and installed (SHA-256
`ED82C46BD98E0F1D07FD6E5075900E42AEDC3FA5E68C06A8764B3DC5303CFF1B`). Current
v143 ATL/Spectre component modification remains blocked at UAC. The v142-based
APO compile is evidence only and is not a production configuration.

## Open prerequisite

Run the ADR 0029 elevated recovery audit for `DESKTOP-AT8EVE9`, retain exact
package/rollback evidence and obtain package-specific approval before B2
deployment. B2T subsequently proved the supported simulated-Broker PCM ingest
and physical Android submission described in `CAPY_AUDIO_001B2T_REPORT.md`;
human-confirmed audibility and the real APO staging producer remain open.
