# CAPY-AUDIO-001B3 Report

Date: 2026-08-27

Status: source, bounded APO/Broker bridge, x64 build and static package
validation complete; signing/deployment not yet run

## Outcome

The reviewed Microsoft SysVAD subset at revision
`717778a20ba4dd2440fe609f69153a1f8a64f597` is imported under MS-PL and reduced
to one root-enumerated render endpoint named `CapyIO Speaker`. The driver
service and binary are `CapyIOAudio`; the endpoint extension registers the
CapyIO-owned render SFX in `CapyIORenderAPO.dll`. Capture, keyword detector,
HDMI, SPDIF, headphone and unrelated APO package dependencies are excluded.

The SFX copies actual 48 kHz stereo float32 render buffers into the fixed
`Local\\CapyIO.RenderRing.v1` staging ring. `APOProcess` performs one bounded
copy, atomics and drop accounting only. Mapping creation/open, validation,
Float32-to-S16 conversion, network transport and failure policy stay in user
mode outside the callback. The exact v1 ABI is in
`drivers/windows-audio/IPC_CONTRACT.md`.

## Toolchain and build evidence

- target: `DESKTOP-AT8EVE9`, AMD64, Windows build `26200.9168`;
- Visual Studio Build Tools 2022 `17.14.51`;
- Windows SDK `10.0.26100.0`;
- WDK `10.0.26100.6584`;
- MSVC v143 x64/x86 Spectre and ATL Spectre components;
- configuration: x64 Release, `SignMode=Off`;
- WIL: pinned revision `3c00e7f1d8cf9930bbb8e5be3ef0df65c84e8928`.

The package project built through MSBuild with a temporary short drive mapping
because upstream source paths exceed several WDK task limits. The WDK embedded
managed InfVerif task reports that its relative `x86\\InfVerif.dll` cannot be
loaded on this installation; this is retained as a tool-integration warning.
The package Signability stage nevertheless completed with zero errors and zero
warnings. The installed x64 native InfVerif tool independently returned zero
for both `/u` and `/w` across all three generated INFs.

## Unsigned package identity

Package directory:
`drivers/windows-audio/sysvad/Package/x64/Release/package`

| File | Bytes | SHA-256 |
|---|---:|---|
| `capyioaudio.cat` | 9501 | `4D7D52E4F79F87819B4D070A80C210BFBA442A9748F498F6FC965F8E995B0E89` |
| `CapyIOAudio.sys` | 79872 | `1248EDEEC280EC5D8E796F0257104BC6897380563DA39E289CFE1D1F36F45E05` |
| `CapyIORenderAPO.dll` | 602624 | `CF3DD5F69931B9E6DA02565F0E5FFA5A5CFA9F27CF72C5BB29001095517BA697` |
| `ComponentizedApoSample.inf` | 2806 | `0C98954DD4532515F50D7645DC1BA172C2B317690DB0ADF7F1BEEB60618C2ECE` |
| `ComponentizedAudioSample.inf` | 3709 | `8C9331104CCED1AF80B269733D0F5B149861A4374276956E864EE2B4FAFBB868` |
| `ComponentizedAudioSampleExtension.inf` | 2645 | `9FF71A01AA2D45508C788CFBEDEA15DF5F3A0E57C778B0BE2AC36F3CC3453671` |

These hashes identify the unsigned build. Signing necessarily changes CAT,
SYS and DLL hashes, so the signed package must receive a second manifest before
installation.

## User-mode tests

The Adapter tests cover mapping exclusivity, exact 128-byte/32-slot layout, a
committed shared-memory block, acquire/release sequencing, bounded
Float32-to-S16 conversion and the existing private Android transport. The test
suite has 15 passing library tests and one intentionally ignored user-supplied
binary probe; binary and supervisor tests also pass.

## Remaining boundary

No certificate was created, no binary was signed, and no driver was installed
or removed. The next action requires exact package approval. It will create a
lab-only test certificate, sign only this package, record signed hashes, create
a fresh restore point, install the three exact INFs, enumerate `CapyIO Speaker`,
run the Android playback smoke, record assigned OEM INF names, and remove only
those names if rollback is required.
