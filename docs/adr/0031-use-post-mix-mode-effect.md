# ADR 0031: Use a post-mix mode effect for the render bridge

Status: accepted

Supersedes: the EFX placement selected by ADR 0028. ADR 0028 continues to own
the bounded user-mode APO/Broker boundary and real-time constraints.

## Context

The installed CapyIO endpoint rendered audio and its meter moved, while the
Broker's APO attach counters remained zero. The component and endpoint devices
were healthy and the effect property store contained the configured endpoint
effect CLSID, so the failure was isolated to graph insertion.

Review of Microsoft's current componentized SysVAD package showed that an
ordinary render endpoint uses stream effects (SFX) before mixing and mode
effects (MFX) after mixing. The sample uses endpoint effects (EFX) for
specialized endpoint processing such as keyword detection. CapyIO had
registered its SFX implementation under the composite EFX property, a graph
position not demonstrated by the render-speaker sample and not instantiated
on the tested Windows build.

An SFX remains unsuitable because Windows can create one instance per input
stream, violating the staging ring's single-producer contract. A composite MFX
runs after stream mixing and provides the required single final render path.
The APO class name is not part of the Windows placement contract; placement is
selected by the endpoint effect property and the implementation already accepts
the common system-effects initialization structures used by MFX.

## Decision

Register the CapyIO render bridge under
`PKEY_CompositeFX_ModeEffectClsid` and declare the supported MFX processing
modes. During in-place upgrades the extension INF removes stale SFX and EFX
properties before adding MFX properties.

The resulting path is:

```text
Windows application streams
  -> Windows endpoint mixer
  -> CapyIO post-mix MFX render bridge
  -> bounded shared-memory/SPSC ring
  -> CapyIO Broker
  -> Android Audio Share transport
```

All callback restrictions and failure behavior from ADR 0028 remain unchanged.
Because this side-channel copy occurs before Windows applies its software
endpoint attenuation, the APO subscribes to endpoint master-volume/mute
notifications and applies the resulting bounded atomic gain only to the copy
written into the render ring. Endpoint lookup and COM calls remain outside the
real-time callback. Legacy initialization without an endpoint collection
retains unity gain and an empty notification registration.

## Consequences

- One post-mix producer preserves the SPSC ring invariant.
- The virtual endpoint's Windows master volume and mute control the PCM sent to
  the remote speaker; CapyIO route gain and the phone's physical volume remain
  separate controls.
- The extension follows the ordinary render placement demonstrated by the
  current Microsoft SysVAD componentized sample.
- A newly signed package and counter-backed playback run must prove MFX
  instantiation before human listening is used as end-to-end evidence.
- If the tested Windows build still does not instantiate the APO, registration
  and activation must be traced before any additional audio transport changes.

## Sources

- Microsoft SysVAD componentized extension INF:
  <https://github.com/microsoft/Windows-driver-samples/blob/main/audio/sysvad/TabletAudioSample/ComponentizedAudioSampleExtension.inx>
- Microsoft SysVAD componentized APO INF:
  <https://github.com/microsoft/Windows-driver-samples/blob/main/audio/sysvad/TabletAudioSample/ComponentizedApoSample.inx>
