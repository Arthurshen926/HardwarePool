# ADR 0050: Align the Windows microphone pin with the capture ring

Date: 2026-09-01

Status: rejected by controlled-host evidence

## Context

The CapyIO microphone endpoint, capture ring and Android native media path use
48 kHz mono float32. The inherited SysVAD `MicIn` pin still advertises eight
16-bit PCM device formats from 8 kHz through 48 kHz. A probe launched from the
Codex filesystem sandbox reached the CapyIO APO, attached the capture-ring
consumer successfully and then failed WASAPI initialization with
`E_INVALIDARG`. Shared and exclusive format matrices failed identically.

That diagnosis was a false negative: the same sandbox probe returned the same
`E_INVALIDARG` for a physical `AB13X USB Audio` microphone. Running the probe
in the interactive `DESKTOP-AT8EVE9\arthu` session opened both devices.

## Rejected decision

The 21.84 experiment changed the CapyIO `MicIn` WaveRT pin to advertise one
device format: 48 kHz, mono, 32-bit IEEE float. Raw, default, speech,
communications and far-field speech modes selected that same device format.
The streaming data range was likewise bounded to 48 kHz mono float32.

This decision is rejected. On `DESKTOP-AT8EVE9`, the signed 21.84 package made
the capture endpoint `NOTPRESENT` and removed it from Windows/CPAL input
enumeration. Removing 21.84 and reinstalling the signed 21.83 package restored
the endpoint without a reboot. In the interactive user session, restored 21.83
then opened for 99 callbacks / 47,520 samples. With Android microphone media
active, the non-retained probe measured RMS `0.00795198` and peak `0.13494873`.
The SysVAD/PortCls WaveRT boundary therefore continues to advertise PCM device
formats.

## Consequences

- 21.84 must not replace 21.83 and remains only as negative lab evidence;
- the kernel pin remains on the known-enumerating PCM contract;
- capture acceptance must run in the interactive user session and include a
  physical-device control before attributing `E_INVALIDARG` to the driver;
- no further APO or kernel-format repair is justified by this observation.
