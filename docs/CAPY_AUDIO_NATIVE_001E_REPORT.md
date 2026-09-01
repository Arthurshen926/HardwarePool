# CAPY-AUDIO-NATIVE-001E implementation report

Date: 2026-08-31

Status: native Speaker transport, service-owned Windows Broker and ordinary
Windows-application playback physically accepted on the controlled lab pair;
release qualification remains

## Implemented path

```text
Windows application
  -> CapyIO Speaker / render APO
  -> bounded Global render ring
  -> capyio-native-virtual-speaker
  -> common 48 kHz stereo S16LE packets / ADR 0044 UDP
  -> Android bounded receiver and reassembly
  -> NativeLanPcmSinkWorker
  -> AudioTrack.WRITE_NON_BLOCKING
```

The protocol-neutral render-ring consumer was extracted into
`capyio-windows-render-ring`; the existing Audio Share Broker imports that
internal crate and retains its behavior. The native backend also includes an
independent 440 Hz sender for isolating Android/network behavior before the
virtual endpoint is involved.

Android trusted-lab authority is compiled from a closed Manifest configuration
and is disabled when no peer environment is supplied. The Activity cannot set
addresses or Route identity. The accepted 0.3.4-dev build is fixed to the
current Windows Tailscale peer, phone UDP port 46000, Windows UDP source port
46001 and the dedicated 001E lab binding.

## Automated evidence

- Android native-LAN contract: PASS — 117 assertions;
- Android lifecycle contract: PASS — 36 assertions;
- Android application compile, warnings-as-errors Lint and debug APK: PASS;
- the Android gate verifies that `MainActivity` and `AudioNodeService`
  compiled classes are present, after physical launch exposed a source-set
  packaging regression in the first 0.3.0-dev artifact;
- native LAN Rust tests: PASS — 12 tests, including tone and render accumulator;
- the native Broker supervisor validates explicit endpoints, bounds retained
  output, waits for a flushed readiness record, reaps the child and rejects
  malformed/missing readiness;
- `CapyIOBroker` accepts a closed `native-speaker` mode and distinguishes
  process/Render-Ring readiness from receiver presence; an active UDP Broker
  therefore does not fabricate `receiverPresent=true`;
- the Windows render ring uses a named, cross-process owner mutex rather than
  mapping existence as liveness, so a mapping retained by `audiodg.exe` can be
  reclaimed after its previous Broker exits while a second live Broker is
  still rejected;
- strict Clippy for native LAN, Windows render ring and Audio Share: PASS;
- Audio Share regression tests pass after render-ring extraction;
- full `cargo xtask ci`: PASS — 185 Rust tests, 4 explicit external/physical
  ignores, strict Clippy, docs/manifests, Adapter smoke and desktop build;
- the current diagnostic APK additionally exposes bounded UDP receipt,
  wrong-peer, malformed-datagram, completed-packet, reassembly-eviction and
  queue-drop counters without per-packet logging.

## Physical evidence to date

1. The authorized 0.3.0-dev APK was installed on V2419A. Its first build
   omitted application entrypoint classes because the Gradle source set
   replaced the default application source directory. Android reported
   `ClassNotFoundException: dev.capyio.android.MainActivity`.
2. The source set and build gate were corrected. The rebuilt application
   launched, retained existing grants and entered Speaker `ACTIVE` with an
   actual 48 kHz, stereo, PCM S16LE `AudioTrack`.
3. An early native tone sender run emitted 1,000 packets / 2,000 datagrams for
   ten seconds. The Android UI still reported zero processed frames.
4. A second early three-second run emitted 300 packets / 600 datagrams. A Tailscale
   capture contained exactly 600 inner UDP datagrams from
   `100.66.231.100:46001` to `100.66.157.119:46000`; Android was listening on
   UDP 46000, but the UI still reported zero frames.
5. After ADB reconnection, 0.3.1-dev diagnostics proved that all 500 packets
   completed reassembly but a 32-entry playback queue dropped 44 packets.
   A 64-entry build varied between 6 and 29 drops under host/Android scheduling
   bursts, even after applying Android's audio thread priority.
6. The accepted 0.3.4-dev build uses the ADR 0045 bounded 128-packet queue. A
   five-second native tone run received exactly 1,000 datagrams, completed all
   500 packets, rendered exactly 240,000 frames and reported zero wrong-peer,
   malformed, reassembly-eviction or queue-drop events.
7. Windows exposed two identically named `CapyIO Speaker with Render Bridge`
   endpoint instances. The endpoint that was initially the default did not
   feed the current Global render ring. Selecting the other ready instance and
   making it the default allowed an ordinary Windows `SoundPlayer` invocation
   of `C:\Windows\Media\Alarm01.wav` to reach the native path. Android advanced
   from 240,000 to 512,160 rendered frames and from 500 to 1,067 completed
   packets (1,000 to 2,134 datagrams), with every error/drop counter remaining
   zero.
8. The installed APK was verified before deployment as version `0.3.4-dev`,
   version code 7, SHA-256
   `7BEDDDBBB6E45D5D8CA612B53BD73AA4E8FB1B643B93D2064324C862F9EBC7DA`.
9. An explicit phone Speaker Stop followed by Start created a fresh generation
   with all frame and network counters reset to zero. Replaying the same
   ordinary Windows WAV then rendered 272,640 frames from 568 completed
   packets / 1,136 datagrams with every error/drop counter still zero.
10. The native Broker was then installed behind the existing `CapyIOBroker`
    service control boundary. `start` first returned `starting`; after the
    configured stable polls, `status` returned `active`, generation 1, with
    `receiverPresent=false` as required for honest UDP readiness.
11. An ordinary `SoundPlayer` WAV through the service-owned Session 0 Broker
    advanced Android from 272,640 to 296,640 frames and from 568 to 618 packets
    (1,136 to 1,236 datagrams), with every error/drop counter remaining zero.
12. Stopping the Windows service released UDP 46001. Restarting it produced a
    fresh stopped generation, then `starting` and `active` on the first start;
    the new Session 0 Broker reacquired 46001. A second ordinary WAV advanced
    Android to 565,920 frames / 1,179 packets / 2,358 datagrams, again with zero
    wrong-peer, malformed, reassembly-eviction or queue-drop events.
13. A clean Release rebuild was deployed into the same service after all source
    changes. The service SHA-256 is
    `684642BBC5EA03FACEFBBD86584FCBF81AA7078468308BAB7B5A393703267A90` and
    the native Broker SHA-256 is
    `D0DD5A5E4D0F46855C76B2C4107CBFF3F2D75F54C6CD9440D8BDB387FF30BF94`.
    That exact build reached `active` and another ordinary WAV advanced Android
    by exactly 24,000 frames / 50 packets / 100 datagrams, to 589,920 frames /
    1,229 packets / 2,458 datagrams, with every error/drop counter still zero.

One diagnostic restart initially encountered Windows socket error 10048. A
control-protocol stop released the previous Broker, an isolated 46002 service
run became active, and the subsequent clean 46001 service stop/start sequence
passed. The normal service-stop regression explicitly proved that 46001 is
released; abrupt process termination and upgrade replacement still need
installer-level qualification.

## Remaining qualification

1. verify an in-flight sender loss and bounded recovery without restarting the
   Android capability;
2. add render-ring attach/block/underrun counters to the native broker's
   machine-readable diagnostics rather than relying only on Android counters;
3. identify and safely retire the stale duplicate Windows endpoint instance in
   the controlled driver-release workflow; friendly name alone is not a safe
   selector;
4. run longer soak, route-change, audio-focus and background/power tests;
5. qualify abrupt service/process termination and signed installer upgrade
   replacement in addition to the now-passing graceful service lifecycle.

The evidence establishes real Windows-application PCM through the CapyIO
virtual endpoint, native transport and Android `AudioTrack`. It does not yet
qualify duplicate-endpoint cleanup, long-duration reliability or subjective
audio quality.
