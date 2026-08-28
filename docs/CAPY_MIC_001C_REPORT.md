# CAPY-MIC-001C endpoint identity and deployment report

Date: 2026-08-28

Status: `21.64.0.1` deployed; `21.65.0.1` ingress-jack build validated, deployment pending

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

`21.61.0.1` attempted to fix this at the Windows AudioEndpointBuilder boundary:

- the Speaker bridge pin has a CapyIO-owned name GUID registered as
  `CapyIO Speaker`;
- the ingress uses a separate topology descriptor whose bridge pin is
  registered as `CapyIO Microphone Ingress`;
- the existing capture bridge-pin GUID is now registered as
  `CapyIO Microphone`;
- the package project preserves an explicitly requested Release configuration,
  and its stamped version matches all three component INFs.

Deployment showed that the registry and kernel parts of that change worked:
the device software key contains all three names and a direct KS property probe
returns `CapyIO Speaker`, `CapyIO Microphone Ingress` and `CapyIO Microphone`.
Windows MMDevice nevertheless continues to display both render endpoints as
localized `Speakers`, including after an AudioEndpointBuilder restart. This is
consistent with Microsoft's documented hard-coded handling of speaker endpoint
categories; `21.61.0.1` therefore does not satisfy endpoint identity acceptance.

`21.62.0.1` kept the dedicated ingress topology and custom Pin Name but changed
its terminal category from `KSNODETYPE_SPEAKER` to the standard
`KSNODETYPE_LINE_CONNECTOR`. Deployment and direct KS probing proved that the
new category was active. Removing and regenerating all three CapyIO MMDevice
instances nevertheless retained the duplicate render names. The standard line
category therefore did not provide the product-specific endpoint identity.

Microsoft's endpoint-name contract distinguishes the Pin Name GUID from the
Pin Category GUID: KS uses the former for `KSPROPERTY_PIN_NAME`, while the
persistent endpoint property store initializes `PKEY_Device_DeviceDesc` from
the category's registered name. `21.63.0.1` therefore assigns the ingress a
CapyIO-owned category GUID and registers that category as
`CapyIO Microphone Ingress`, while retaining the independent Name GUID.
Deployment proved all of those registrations and produced new MMDevice IDs,
but the display prefix remained `Speakers`. Registry evidence isolated the
remaining mismatch: every topology interface still declared
`PKEY_AudioEndpoint_Association=KSNODETYPE_ANY`. `21.64.0.1` explicitly
associates Speaker, Microphone Ingress and Microphone with their actual Pin
categories, as required by the documented endpoint-property contract.
Deployment proved that the explicit association reached MMDevice, but the
display prefix still remained `Speakers`. The ingress topology was still using
the Speaker automation table, which reports an integrated stereo-speaker jack.
`21.65.0.1` removes Speaker jack properties from the private ingress topology;
the actual CapyIO Speaker retains them unchanged.

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

Signed `21.64.0.1` is installed on `DESKTOP-AT8EVE9`; all five CapyIO devices
and the three audio/Broker services are healthy. Before deploying `21.65.0.1`,
the exact package must be built, staged, hashed and lab-signed, and signed
`21.64.0.1` must be retained as the immediate rollback target.

## Remaining acceptance work

1. Build, sign and deploy `21.65.0.1`, then verify distinct Speaker and
   Microphone Ingress MMDevice names.
2. Play a deterministic 48 kHz tone into Microphone Ingress and record it from
   CapyIO Microphone through an ordinary WASAPI client.
3. Verify Broker absence and ingress disconnect return capture to silence.
4. Build/probe pinned MicYou v2.0.1, then install its APK only for the approved
   physical Android test and retain the Android-to-Windows recording evidence.
