# ADR 0051: Disable synthetic Windows render loopback

Date: 2026-09-01

Status: accepted on controlled host

## Context

`CapyIO Speaker` inherits the SysVAD offload render topology. The upstream
sample advertises a WASAPI loopback pin, but it does not mix the real host and
offload streams. Its loopback stream generates a synthetic sine tone instead.
CapyIO already rejects that signal as media evidence and obtains real post-mix
render PCM from the endpoint-associated APO and bounded render ring.

On `DESKTOP-AT8EVE9`, NetEase UU Remote 4.38.3.9325 installs its own render and
capture endpoints and records host audio. With only the CapyIO speaker Route
active, the Android microphone remained `STOPPED` with zero frames and packets,
while eight seconds of ordinary Windows playback reached Android as 384,000
frames, 800 packets and 1,600 datagrams without transport errors. UU still
produced a high-frequency sound. This excludes CapyIO full-duplex feedback and
the Android/network media path; the remaining source is the inherited
synthetic system-loopback stream opened by the remote-recording application.

## Decision

Add an endpoint flag, `ENDPOINT_SYNTHETIC_LOOPBACK_DISABLED`, and set it only
on `CapyIO Speaker`. The WaveRT miniport then reports loopback unsupported and
rejects creation of the inherited loopback capture stream with
`STATUS_NOT_SUPPORTED`. Offload/system render pins and the APO render-ring path
remain unchanged.

The 21.85 package is a build candidate. It must not be installed until the
controlled-host preflight records the exact signed package, existing 21.83
rollback packages and recovery command. Acceptance must prove:

- ordinary Windows playback still reaches the Android speaker Sink;
- Windows volume remains effective;
- the Android microphone can remain independently stopped or active;
- UU Remote no longer receives a synthetic tone from `CapyIO Speaker`;
- service restart and 21.83 rollback work without a system reboot.

The x64 Release driver/APO build passed `/W4 /WX` and Universal API validation.
Independent WDK 10.0.26100.6584 `InfVerif.exe /u` and `/w` checks passed for all
three generated INFs, and `Inf2Cat` completed with no errors or warnings. The
retained unsigned archive is 302,135 bytes with SHA-256
`73DF9019F3811A8F8F0991720A9F352EF5FC3655DDD2A420D5788FF733726EEC`.
The known embedded WDK `x86\InfVerif.dll` loader defect remains visible and is
not used as package evidence.

The package was signed by `CapyIO Driver Lab 84f16b8` and installed without a
reboot as `oem179.inf`, `oem180.inf` and `oem181.inf`. The retained 21.83
rollback packages remain `oem176.inf`, `oem177.inf` and `oem178.inf`. Windows
reported one healthy render endpoint and one healthy capture endpoint, while
`AudioEndpointBuilder`, `Audiosrv` and `CapyIOBroker` all returned to Running.

A direct WASAPI loopback initialization on `CapyIO Speaker` now fails with
HRESULT `0x88890008` instead of opening the synthetic stream. Five seconds of
ordinary Windows playback still added about 240,480 Android speaker frames;
the phone reported zero wrong-source, malformed, reassembly-eviction and queue-
drop counters. A normal Windows capture client concurrently opened the CapyIO
microphone for every 100 ms window of a five-second run and measured non-zero
RMS/peak without retaining audio. Both directions also ran concurrently, then
stopped cleanly while the Windows services remained running. The owner then
confirmed through UU Remote that Android playback remained normal and the
previous high-frequency squeal was absent. This human listening result closes
the controlled-host compatibility criterion; it is not treated as an automated
acoustic-quality measurement.

Keeping UU Remote running exposed a separate local-lab port collision. Winsock
rejected the complete tested 45980--46030 UDP interval with error 10048 even
when no owner appeared in `Get-NetUDPEndpoint` or elevated `netstat`; an unused
46012 failed identically, while 40000/40001/40010/40011 all bound normally.
This is not evidence of a leaked CapyIO socket. The controlled build and
`CapyIOBroker` configuration were therefore moved to phone/Windows speaker
ports 40000/40001 and microphone ports 40010/40011 without weakening exclusive
bind semantics or adding a networking dependency. With UU still running,
generation 3 stopped with both children and ports absent, and generation 4
restarted with new speaker/microphone PIDs owning 40001/40011. Android then
reported both Routes `ACTIVE`: microphone sent 6,016 packets/datagrams with no
queue drop, and speaker had received 34,614 datagrams / 17,307 complete packets
with no wrong-source, malformed, reassembly-eviction or queue-drop event.
Windows microphone sampling was non-zero in every reported 100 ms window.

## Consequences

- remote-recording, streaming and conferencing applications cannot capture
  `CapyIO Speaker` through WASAPI loopback in this slice; activation fails
  rather than returning fabricated audio;
- this is a general endpoint safety rule, not a UU-specific process block;
- the phone-bound CapyIO media path continues to use the real-time APO bridge;
- genuine Windows loopback compatibility remains a separate feature. It needs
  a bounded real post-mix mirror with explicit offload, volume, mute, format,
  protected-content and multi-client semantics; it must not reuse SysVAD's
  tone generator or put network behavior in the kernel driver.
- fixed lab ports are deployment configuration, not protocol constants; a
  release UI must detect local conflicts or negotiate ports before activation.
