# CAPY-AUDIO-001B0 Report

Date: 2026-08-25

Status: product/architecture and candidate audit complete; isolated target provisioning in progress

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
  -> existing Audio Share Adapter-managed transport
  -> Android speaker
```

This avoids adding networking or a custom PCM ring to the kernel driver. The
existing trusted endpoint picker is reusable once `CapyIO Speaker` enumerates.

The APO callback is limited to a preallocated, non-blocking copy/drop path. The
exact shared-memory setup is still an isolated-target spike, not a proven ABI.

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
- daily host: x64 Windows build 26200.8875;
- Visual Studio Build Tools 17.14 and Windows SDK 10.0.26100.0 present;
- WDK MSBuild integration and InfVerif not found;
- upstream build metadata requires the WDK, v142 x64 tools, ATL and Spectre
  libraries; an attempted toolchain install did not complete, so none of these
  components is accepted as available evidence;
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
- Windows guest installation has begun, but first boot, baseline checkpoint
  and toolchain evidence are still pending.

The first direct GitHub query timed out without a proxy; the read-only pinned
revision query succeeded through the user-provided Clash proxy at
`127.0.0.1:7897`.

## Safety and validation

No WDK driver tool, driver install/remove, signing command,
BitLocker/test-signing/Verifier operation or source import occurred. VM creation,
Secure Boot/vTPM configuration and guest OS installation are confined to the
identified `CapyIO-DriverLab` target.
Repository validation now requires the SysVAD and candidate revision/license
records, the `CapyIO Speaker`/APO/bounded/isolated-target boundary, and rejects
driver source while the provenance record remains `source_imported: false`.

The source archive was downloaded and inspected only under the ignored
`.agent-cache` directory. The Microsoft-signed WDK 26100.6584 bootstrapper was
also cached and verified (SHA-256
`ED82C46BD98E0F1D07FD6E5075900E42AEDC3FA5E68C06A8764B3DC5303CFF1B`), but
was not executed. A Visual Studio component modification was stopped after its
log showed channel access failure; the proxy-assisted retry was cancelled at
UAC. The repository therefore records the toolchain as incomplete.

## Open prerequisite

Complete Windows first boot in `CapyIO-DriverLab`, detach the installer ISO,
create and retain a clean baseline checkpoint, then install the pinned guest
toolchain for `CAPY-AUDIO-001B1`. The daily host cannot be used as a substitute.
