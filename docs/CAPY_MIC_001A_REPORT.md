# CAPY-MIC-001A enumeration baseline report

Date: 2026-08-28

Status: source/build complete; not deployed and not functionally complete

## Outcome

The existing CapyIO SysVAD derivative now compiles one additional capture
miniport whose intended Windows endpoint name is `CapyIO Microphone`. The same
root driver retains the proven `CapyIO Speaker`; unrelated SysVAD capture
arrays, keyword paths and network/media logic remain absent.

This baseline deliberately still inherits SysVAD's capture test-tone generator.
It must not be described or tested as the finished remote microphone. The next
slice replaces that tone at the capture APO boundary with bounded Broker PCM
and silence on underflow.

## Changed source

- `TabletAudioSample/minipairs.h`: one capture endpoint and corrected miniport
  count;
- `TabletAudioSample/micintoptable.h`: CapyIO-owned connector-name GUID;
- `TabletAudioSample/TabletAudioSample.vcxproj`: minimal microphone topology
  implementation compile item;
- `TabletAudioSample/ComponentizedAudioSample.inx`: capture interfaces,
  friendly names and package version `0.2.0.0`.

## Build evidence

- target/build host: `DESKTOP-AT8EVE9`, AMD64;
- Visual Studio Build Tools 2022 `17.14.51`;
- Windows SDK `10.0.26100.0`;
- WDK `10.0.26100.6584`;
- configuration: x64 Release, `SignMode=Off`;
- MSVC kernel compile: `/W4 /WX`, passed;
- link and Universal Driver API validation: passed;
- independent x64 InfVerif `/u`: exit 0;
- independent x64 InfVerif `/w`: exit 0.

The already-recorded WDK installation defect caused its embedded managed task
to emit three `x86\\InfVerif.dll` loader errors. It did not stop compile/link,
and the separately installed native x64 verifier passed both modes.

## Safety and missing evidence

No signing, catalog generation, driver install/update/removal, endpoint
enumeration or capture was performed. Those actions are intentionally deferred
until the capture ring/APO path can replace the sample tone; installing this
intermediate package would not provide useful phone audio.
