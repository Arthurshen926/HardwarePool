# CAPY-AUDIO-NATIVE-001G1 partial-failure isolation report

Date: 2026-09-01

Status: symmetric child-exit isolation physically accepted; full concurrent
media 001G acceptance pending

## Implemented behavior

ADR 0048 separates native pair liveness from readiness in the Windows Broker.
Startup remains transactional, while a later speaker-only or microphone-only
child exit no longer causes the service Runtime to stop the surviving child.
The service leaves `active`, returns to `starting`, retains no false receiver
presence and waits for an explicit recovery operation. Both children exiting
still fails and reaps the pair through the existing bounded path.

The Android Node already has independent generation-bound microphone Source
and speaker Sink state. Its contract tests prove concurrent activation,
one-direction Stop, single-direction failure, later-generation Retry and stale
completion rejection. No Android lifecycle or permission behavior changed in
this slice.

## Automated evidence

- `native_pair_health_preserves_a_surviving_direction` proves partial health
  neither stops nor misclassifies the healthy child;
- `transport_readiness_loss_keeps_the_running_process_alive` proves readiness
  loss leaves the process live, clears no unrelated Route and publishes no
  terminal Problem;
- the transactional microphone-start rollback regression remains passing;
- the Windows service crate has 19 passing unit tests after this change.

The focused crate tests, strict Clippy, Android Gate and repository-wide
`cargo xtask ci` all pass. Android Gate retains 36 lifecycle assertions and
126 native-LAN assertions.

## Controlled physical evidence

The deployed Broker SHA-256 is
`C8A917F92134AF2F4DD3266FA319F746B61C50A909027B5FFD987A47870055BE`.
The unchanged speaker and microphone child hashes remain respectively
`1E26D822269DC4882664EE50BAB95A06CEC9F097C569D4F3C7F3ED9FAD2E97AC`
and
`2A826EC4FA4C4F996FC0F15BD6705FE65DC4E134D18387304BCB375F778001AD`.
The controlled Android 0.4.2-dev build used for partial-exit isolation has SHA-256
`31FB4A857E5ABF01EB9B19AA33F7E447AE912F28117F9435782B46A264F585D9`.

Both Android capabilities reached `ACTIVE`. Before the partial-exit tests the
microphone Source had processed 2,526,720 frames and sent 5,264 packets with
zero queue drops. During the test it continued to 35,481,600 frames and 73,924
packets with zero queue drops.

In generation 3, terminating only speaker PID 81924 released UDP 46001 and
moved the Broker from `active` to `starting`. Microphone PID 78996 retained UDP
46011 unchanged and continued sending. Explicit Stop/Start restored both
children in generation 4.

In generation 4, terminating only microphone PID 80584 released UDP 46011 and
moved the Broker from `active` to `starting`. Speaker PID 83900 retained UDP
46001 unchanged. Explicit Stop/Start restored both children in generation 5,
which finished `active` with speaker PID 68692 and microphone PID 74560.

The Android microphone and speaker were then stopped independently; both
reported `STOPPED` and the service had `startRequested=false`. No reboot,
driver replacement or security-policy change occurred.

## Follow-up diagnosis

This run did not accept full-duplex media. The installed Windows audio device
currently exposes one CapyIO microphone endpoint and two same-name CapyIO
speaker endpoints. PnP reports all as `OK`, but every tested shared/exclusive
WASAPI microphone initialization returns `0x80070057`, and explicit output
streams on both speaker ordinals produce no render-ring packets. Restarting
Windows Audio with the Runtime both stopped-first and already-active did not
repair the endpoints.

The phone-inbound conclusion from the first run was incorrect. Android was
receiving and rendering packets, but the foreground Activity did not request
another metrics snapshot after activation. Version 0.4.3-dev now schedules a
coalesced 250 ms refresh only while either capability owns foreground lifecycle.
Without rebinding the Activity, a five-second native sender run received exactly
1,000 datagrams, completed 500 packets and rendered 240,000 frames with zero
wrong-peer, malformed, reassembly-eviction or queue-drop events.

The two Windows render endpoints were then identified by their MMDevice KS
paths: one is the real `WaveSpeaker`; the other is ADR 0037's obsolete
`WaveMicIngress`, not a random stale driver copy. The same parent-device APO
selects the first recognized association in its device collection. When it
selects ingress before capture, a microphone graph enters the render-format
path and WASAPI returns `0x80070057`. ADR 0049 removes the native runtime's
ingress endpoint and producer role. The unsigned 21.83 x64 Release package
passes compilation, INF verification, signability and catalog generation;
signed installation and full-duplex physical acceptance were pending at this
point in the run.

## 21.83 endpoint-convergence follow-up

The signed 21.83 package was subsequently installed and now exposes one active
CapyIO speaker plus one active CapyIO microphone. The first post-install
WASAPI probes were launched from the Codex filesystem sandbox and returned
`0x80070057`; a physical USB microphone returned the same error in that
account. This was a harness false negative, matching the earlier 21.82
qualification finding.

In the interactive `DESKTOP-AT8EVE9\arthu` session, the physical microphone
opened for 98 callbacks / 94,080 samples and the restored 21.83 CapyIO
microphone opened for 99 callbacks / 47,520 samples. With the CapyIO Android
microphone Source and native Runtime active, the non-retained CapyIO probe
measured RMS `0.00795198` and peak `0.13494873`.

A signed 21.84 experiment that advertised a float32 WaveRT pin made the
capture endpoint `NOTPRESENT`. It was removed and the signed 21.83 package was
restored without reboot. ADR 0050 retains that negative evidence and rejects
the float-pin design.

## Retained work

- repeat simultaneous phone render and Windows capture after 21.83 with both
  direction counters retained in one evidence bundle;
- exercise phone/network interruption, explicit recovery and both Stop orders;
- add bounded per-direction restart/backoff and direction-specific diagnostics;
- test microphone permission revoke and speaker audio-focus/routing changes;
- retain the trusted-LAN security restriction from ADR 0044.
