# CAPY-AUDIO-001B3 Report

Date: 2026-08-27

Status: endpoint, package and cross-session mapping proven; diagnostic playback
isolated the remaining graph-position defect and selected a post-mix APO fix

## Outcome

The reviewed Microsoft SysVAD subset at revision
`717778a20ba4dd2440fe609f69153a1f8a64f597` is imported under MS-PL and reduced
to one root-enumerated render endpoint named `CapyIO Speaker`. The driver
service and binary are `CapyIOAudio`; the endpoint extension registers the
CapyIO-owned render APO in `CapyIORenderAPO.dll`. Capture, keyword detector,
HDMI, SPDIF, headphone and unrelated APO package dependencies are excluded.

The post-mix MFX copies actual 48 kHz stereo float32 render buffers into the fixed
`Global\\CapyIO.RenderRing.v1` staging ring. `APOProcess` performs one bounded
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

These hashes identify the unsigned build.

## Lab-signed package identity

The approved signing-only step created a non-exportable local-machine code
signing key with subject `CN=CapyIO Driver Lab f29c8a5`, SHA-1 thumbprint
`7353E729B92ACD148BB0046FB254E976A4762FEF`, valid through 2027-08-27. Its
public certificate is present in the exact host's Local Machine Root and
Trusted Publishers stores. It is a lab-only self-signed identity, not a
production or Microsoft-rooted certificate.

The SYS and APO DLL were signed first, Inf2Cat then regenerated the catalog
with zero errors and zero warnings, and the catalog was signed last. Final
package hashes are:

| File | Bytes | SHA-256 |
|---|---:|---|
| `capyioaudio.cat` | 11182 | `4574EA3CB5554F0FCD9B942FB5AD7BAC5FDCF0CB225CBD629520EBFD18A8D80C` |
| `CapyIOAudio.sys` | 81688 | `E14E9FD0791698F2BD0C5C826277654A3D4507D47517538496CA1200C404EB7F` |
| `CapyIORenderAPO.dll` | 604440 | `21A9FC6D6AA5C45FB87ADFA97522E0F00F0907BF5880D2CDA47E7A72E0D3C723` |
| `ComponentizedApoSample.inf` | 2806 | `0C98954DD4532515F50D7645DC1BA172C2B317690DB0ADF7F1BEEB60618C2ECE` |
| `ComponentizedAudioSample.inf` | 3709 | `8C9331104CCED1AF80B269733D0F5B149861A4374276956E864EE2B4FAFBB868` |
| `ComponentizedAudioSampleExtension.inf` | 2645 | `9FF71A01AA2D45508C788CFBEDEA15DF5F3A0E57C778B0BE2AC36F3CC3453671` |

SignTool Authenticode verification passes for the SYS, DLL and CAT, and the
catalog-membership check passes for the SYS. Independent InfVerif `/u` and
`/w` still return zero. Kernel-policy `/kp` verification reports the expected
lab limitation that this self-signed certificate does not chain to a Microsoft
root; installation therefore relies on the already-enabled Windows test-signing
mode and is not evidence of production driver-signing eligibility.

## User-mode tests

The Adapter tests cover mapping exclusivity, exact 128-byte/32-slot layout, a
committed shared-memory block, acquire/release sequencing, bounded
Float32-to-S16 conversion and the existing private Android transport. The test
suite has 15 passing library tests and one intentionally ignored user-supplied
binary probe; binary and supervisor tests also pass. The Windows-only
`capyio-render-ring-probe` helper performs a one-shot open/map/header check so
the lab can verify the AudioDG service identity's access without adding file or
logging work to the real-time callback.

## Deployment and remaining boundary

The exact signed package above was installed on `DESKTOP-AT8EVE9` after restore
point sequence 374 was created. The base, APO and extension packages were
assigned `oem70.inf`, `oem73.inf` and `oem74.inf`. The root MEDIA device,
software APO component and `CapyIO Speaker with Render Bridge` audio endpoint
all enumerate with status OK.

Pre-playback inspection then found AudioDG in Session 0 while the interactive
Broker owns a per-user session. The original `Local\\` mapping therefore could
not support the promised end-to-end path. The source now uses a protected
`Global\\` mapping, explicitly grants AudioDG's Local Service identity access,
and exposes bounded-duration counters for objective lab evidence. That corrected
package was signed as commit `4b2407a`, installed as `oem75.inf`, `oem76.inf`
and `oem77.inf`, and selected by the base and APO devices. Restore point 375
was created before the update; the MEDIA device, APO component and render
endpoint remained status OK.

The Windows control-panel test then showed active endpoint levels and Android
held an established TCP session with a started 48 kHz stereo `AudioTrack`, but
the bounded Broker evidence correctly reported `ring_produced=0`. Elevated
module inspection proved `CapyIORenderAPO.dll` was loaded by Session 0
`audiodg.exe` under Local Service. Source inspection then identified the actual
failure: the endpoint's first/default engine format was 44.1 kHz while the
bounded ring deliberately attaches only at the 48 kHz speaker baseline.

The format table now makes 48 kHz stereo the default and retains 44.1 kHz as a
non-default compatibility entry. A diagnostic package then proved that the
Broker-owned global mapping is openable and mappable by Local Service, while
the endpoint meter moved but every APO attach counter remained zero. The SFX
was therefore not inserted into the active graph. It was also architecturally
incorrect for the single-producer ring because Windows may create one SFX per
input stream. An EFX placement was not instantiated on the tested ordinary
render endpoint. ADR 0031 therefore binds the APO as the post-mix MFX and
accepts the declared default, media and movie processing modes. The
first in-place update exposed one more upgrade rule: Windows retained old effect
property values. The extension now explicitly deletes obsolete SFX and EFX
properties before adding MFX PID 14/6. A newly signed update and repeated
physical test are still required; no successful
end-to-end virtual-speaker claim is made from the zero-block diagnostic run.

The first MFX package then produced decisive crash evidence: Windows inserted
and locked the APO at 48 kHz stereo, but each render start terminated
`audiodg.exe` with access violation `0xc0000005` at DLL RVA `0xCE8F`.
Disassembly with the matching PDB identified
`CSwapAPOSFX::GetApoNotificationRegistrationInfo`: the retained SysVAD sample
notification code queried a null `m_audioEndpoint` after the bridge initializer
had deliberately stopped creating endpoint/property-store dependencies. The
bridge has fixed effect state and consumes no property notifications, so it now
returns a valid empty notification descriptor list. Signed package `2a9a643`
then kept the same AudioDG process alive through playback, advanced the ring to
`ring_produced=547` with `ring_dropped=0`, reported two successful 48 kHz stereo
attachments and retained an established Android receiver session. Android
reported a started 48 kHz stereo `AudioTrack`, and the human operator confirmed
that the Windows control-panel test was audible from the phone. This establishes
the first physical virtual-endpoint-to-phone proof.

That proof does not yet establish product lifecycle or clean long-duration audio.
The validation launcher supplied a 300-second Broker duration; after it expired,
Windows correctly retained the virtual endpoint but there was no user-mode process
to drain and forward its render ring. A duration-free lab Broker restored an
established TCP/UDP receiver session and remains required until the Windows Node
Runtime owns this Broker's lifecycle. The pinned Android receiver was also found
using its default `1x` `AudioTrack` buffer and non-blocking per-datagram writes;
Android AudioFlinger recorded receiver-track underruns during the physical run.
The lab receiver buffer was raised to `4x` for follow-up listening. This is a
latency-for-stability mitigation, not a substitute for a CapyIO-framed transport,
ordered receive queue and jitter buffer.

A later long-running check exposed a separate reconnect defect. After Android
restarted the Audio Share process, TCP re-established but both newly created
Android tracks retained zero written frames. A default-device WAV advanced the
Windows ring from 30,965 to 31,342 blocks with zero drops and a successful APO
reattach, proving that capture and ring drain remained healthy. The stale Android
UDP port can cause Windows to surface an ICMP port-unreachable response as
`ConnectionReset` or `ConnectionRefused` on the shared registration socket. The
registration worker previously treated every error other than `WouldBlock` as
terminal, so later UDP registration datagrams were never processed even though
TCP appeared established. The worker now treats reset, refused and timed-out UDP
receives as transient. A physical regression force-stopped Android PID 2331,
restarted the receiver as PID 5336 without restarting Broker PID 22916, and then
advanced the new Android 48 kHz stereo track to `0x36630` server frames from a
second Windows default-device WAV. This proves recovery across a changed Android
process and UDP source port.

## Endpoint-volume follow-up

Physical listening then exposed that changing the `CapyIO Speaker` Windows
master volume did not change the phone playback level. The post-mix MFX was
copying its input into the side-channel ring before the software endpoint
attenuation owned by Windows. The source now caches the endpoint during
non-real-time initialization, subscribes only to endpoint-volume notifications,
and stores mute/master volume as one bounded atomic fixed-point gain. The
real-time callback snapshots that value and applies it during the existing ring
copy without COM, allocation, locks, logging, file or network work. This source
and build change still requires a newly approved exact-package deployment and
physical checks at 100%, an intermediate setting, 0%/mute and restored 100%
before endpoint-volume behavior is claimed as proven.

The x64 Release APO rebuild passed MSVC `/W4 /WX` and Windows Driver API
validation with zero warnings and zero errors. A full unsigned package rebuild
also completed Inf2Cat signability with no errors or warnings. The WDK embedded
InfVerif task retained the already-recorded three `x86\\InfVerif.dll` loader
errors; this is a local WDK integration limitation, not a clean InfVerif result.
The exact unsigned pre-signing archive is retained outside version control as
`.agent-cache/capyio-volume-fix-unsigned-20260827.zip` (297,934 bytes, SHA-256
`E571C28306EFE86993B71DB407B2D8A8E2F616E331274F8A9037CE0AA7655FDC`).

| Unsigned file | Bytes | SHA-256 |
|---|---:|---|
| `capyioaudio.cat` | 9673 | `947436FCA715381926EAAAA913B0FEFE8FC7430CA934552D7FAF24F27219F95C` |
| `CapyIOAudio.sys` | 79872 | `F46BAD4870E263E17F11D5AD2E2BF4860D99624EADA744D3435DE74B8C7ADEE2` |
| `CapyIORenderAPO.dll` | 601600 | `26B95DCDC60D1D03FAB6A10511AF37AA00B6D2452F9F384EA9B3F363138656FD` |
| `ComponentizedApoSample.inf` | 2806 | `EF4FA4003B47CC02090BD6ECB49287C8471BA175EE08EDE3AD127F80F830988F` |
| `ComponentizedAudioSample.inf` | 3709 | `7968E23239E40AD834E0FD1C8C39FBF4E0C43D0F149C4DD3D35A99BC7E040363` |
| `ComponentizedAudioSampleExtension.inf` | 3296 | `C21F5E090E48008D9DAE673EA44EF6AE62DCFB7CD52A4DFBBCE1511E05F776A1` |
