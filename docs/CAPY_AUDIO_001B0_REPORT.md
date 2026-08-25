# CAPY-AUDIO-001B0 Report

Date: 2026-08-25

Status: product/architecture realignment complete; isolated target access open

## Outcome

The Windows speaker target is now explicitly an independent system render
device named `CapyIO Speaker`, not permanent mirror mode. ADR 0027 separates
the proven Audio Share transport into Gate 7A and the driver-backed projection
into Gate 7B.

The chosen first architecture is:

```text
Windows application
  -> CapyIO Speaker (minimal SysVAD-derived WaveRT endpoint)
  -> user-mode WASAPI loopback
  -> existing Audio Share Adapter-managed transport
  -> Android speaker
```

This avoids adding networking or a custom PCM ring to the kernel driver. The
existing trusted endpoint picker is reusable once `CapyIO Speaker` enumerates.

## Provenance and local inventory

- Microsoft Windows-driver-samples pinned at
  `717778a20ba4dd2440fe609f69153a1f8a64f597`;
- upstream path `audio/sysvad`, license MS-PL;
- no upstream source or binary imported;
- daily host: x64 Windows build 26200.8875;
- Visual Studio Build Tools 17.14 and Windows SDK 10.0.26100.0 present;
- WDK MSBuild integration and InfVerif not found;
- Hyper-V VM Management is running, but the current account lacks permission
  to enumerate VMs.

The first direct GitHub query timed out without a proxy; the read-only pinned
revision query succeeded through the user-provided Clash proxy at
`127.0.0.1:7897`.

## Safety and validation

No WDK driver tool, driver install/remove, signing command, VM mutation,
Secure Boot/BitLocker/test-signing/Verifier operation or source import occurred.
Repository validation now requires the SysVAD revision/license record, the
`CapyIO Speaker`/WASAPI/isolated-target boundary, and rejects driver source while
the provenance record remains `source_imported: false`.

## Open prerequisite

`CAPY-AUDIO-001B1` requires one exact Hyper-V Generation 2 VM or dedicated
Windows installation with snapshot/recovery planning. The daily host cannot be
used as a substitute.
