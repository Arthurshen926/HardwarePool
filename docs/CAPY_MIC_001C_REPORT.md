# CAPY-MIC-001C endpoint identity and deployment report

Date: 2026-08-28

Status: `21.60.0.1` deployed; `21.61.0.1` source/build validated, deployment pending

## Outcome

The approved local-lab deployment of signed `21.60.0.1` proved that the paired
driver package can be installed and restarted without a system reboot. The
Speaker render endpoint, microphone ingress render endpoint, microphone capture
endpoint, render APO and root MEDIA device all enumerated with healthy PnP
status. `AudioEndpointBuilder`, `Audiosrv` and `CapyIOBroker` remained running.

The deployment also exposed an endpoint-identity defect: both render endpoints
were displayed as `Speakers (CapyIO Speaker with Render Bridge)`. The capture
endpoint used the generic `Microphone` connector name. MicYou v2.0.1 selects an
output device by exact friendly name, so duplicate render names block a safe,
deterministic Adapter configuration even when both devices are operational.

`21.61.0.1` fixes this at the Windows AudioEndpointBuilder boundary:

- the Speaker bridge pin has a CapyIO-owned name GUID registered as
  `CapyIO Speaker`;
- the ingress uses a separate topology descriptor whose bridge pin is
  registered as `CapyIO Microphone Ingress`;
- the existing capture bridge-pin GUID is now registered as
  `CapyIO Microphone`;
- the package project preserves an explicitly requested Release configuration,
  and its stamped version matches all three component INFs.

The root device suffix can remain visible in localized Windows UI. The
application-facing endpoint prefix is now independent and the two render
devices no longer depend on an ambiguous generic `Speakers` label.

## Build evidence

- target: Windows 11 build `26200`, x64, `DESKTOP-AT8EVE9`;
- Visual Studio Build Tools/MSBuild `17.14.51`;
- Windows SDK/WDK `10.0.26100.0`, KMDF `1.15`;
- Release APO and kernel builds: `/W4 /WX`, zero compiler warnings/errors;
- Universal API validation: passed for the APO and kernel driver;
- Inf2Cat Signability: zero errors and zero warnings;
- independent x64 InfVerif `/u` and `/w`: passed for all three INFs;
- repository structural validation: passed;
- `capyio-windows-service`: 9 tests passed;
- `capyio-micyou-adapter`: 6 tests passed, 1 physical-CLI test ignored;
- Clippy with warnings denied passed for both affected Rust crates.

The WDK MSBuild wrapper continues to print its known managed-task failure while
loading relative `x86\\InfVerif.dll`. It does not fail the build, and the native
x64 verifier is the retained INF evidence.

## Deployment boundary

No `21.61.0.1` driver was installed by this source/build slice. Before deployment
the exact package must be staged, hashed and lab-signed, the `21.60.0.1` package
must remain the verified rollback target, and the human must approve that exact
package for `DESKTOP-AT8EVE9`.

## Remaining acceptance work

1. Deploy signed `21.61.0.1` and verify three distinct endpoint names.
2. Play a deterministic 48 kHz tone into Microphone Ingress and record it from
   CapyIO Microphone through an ordinary WASAPI client.
3. Verify Broker absence and ingress disconnect return capture to silence.
4. Build/probe pinned MicYou v2.0.1, then install its APK only for the approved
   physical Android test and retain the Android-to-Windows recording evidence.
