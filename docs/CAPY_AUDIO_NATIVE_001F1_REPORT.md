# CAPY-AUDIO-NATIVE-001F1 implementation report

Date: 2026-08-31

Status: native microphone media data plane physically proven; matching service
deployment and ordinary Windows-client recording subsequently accepted in
`docs/CAPY_AUDIO_NATIVE_001F2_REPORT.md`

## Implemented path

```text
Android microphone
  -> AudioRecord bounded read worker
  -> common 48 kHz mono S16LE packets / bounded sender queue
  -> ADR 0044 explicit-peer UDP
  -> capyio-native-virtual-microphone
  -> bounded Global capture ring
  -> CapyIO virtual microphone / ordinary Windows client
```

`capyio-windows-capture-ring` now owns the fixed mapping ABI and live-owner
claim. `MicrophoneSourceAdapter` composes the existing packetizer, bounded
queue, UDP endpoint and sender worker rather than performing network work on
the audio worker. Its closed Manifest configuration is fixed to the lab Route
and is unavailable to the Activity.

The Windows native service configuration can now carry both speaker and
microphone child processes. Microphone configuration is all-or-none and valid
only in native mode. Startup is transactional: microphone failure stops the
already-started speaker child. Runtime health requires both children.

## Automated evidence

- capture-ring crate: 4 tests pass, covering fixed zeroed layout, live owner
  and stale reclaim, exact S16LE-to-float32 conversion/commit, full-ring block
  drop and malformed PCM rejection;
- native LAN: 17 tests pass across supervisor/library/binaries/integration,
  including the exact microphone binding, UDP loopback and microphone-specific
  readiness prefix;
- Windows service: 17 tests pass, including complete/partial/mode-incompatible
  microphone launch configuration and speaker rollback on microphone failure;
- affected Rust crates pass strict warnings-as-errors Clippy;
- Android lifecycle contract: 36 assertions pass;
- Android native-LAN contract: 126 assertions pass;
- Android compile, warnings-as-errors Lint and debug APK assembly pass.

## Physical evidence

The authorized `0.4.0-dev` APK (version code 8, SHA-256
`3E5D1639367EC5FA72F2E803CE54A2CE7F74DB6E8D28F92674AC02ABC7F75786`)
was installed on the controlled Android device. It used phone UDP 46010 and
Windows UDP 46011 with the dedicated microphone binding.

With MicYou processes absent, a 45-second native Windows receiver run observed:

- 477 packets / 477 datagrams received from the configured peer;
- 228,960 microphone frames committed to the capture ring;
- zero wrong-peer, malformed, full-ring or ring-frame drops;
- one producer attach and one Windows capture consumer attach;
- capture-ring last error zero.

This proves real `AudioRecord` PCM traversed the CapyIO common packet and native
UDP backend into the Windows capture projection without the MicYou Android app
or MicYou transport.

The initial ordinary-client recording attempt correctly failed closed
before readiness because the still-running installed Broker predates the new
capture owner mutex. Source and deployed binary were intentionally not made
ABI-inconsistent by bypassing that check. The combined service build and a
recoverable hot-update script are prepared, but Windows did not grant the tool
process a service-control administrator token in this run. Ordinary-client WAV
capture was retained as the final 001F2 deployment acceptance item and later
passed with the exact matching Release. See the 001F2 report for hashes, WAV
metrics and lifecycle evidence.

## Remaining qualification

1. deploy the matching service, speaker and microphone binaries, then prove a
   normal Windows recording client reports non-zero samples/RMS/peak;
2. verify native microphone Stop/Start, phone loss, bounded failure and fresh
   recovery while the speaker Route remains independent;
3. expose direction-specific service state and bounded media/ring counters to
   Desktop instead of relying on child stdout and lab UI counters;
4. enforce a common producer claim in every retained compatibility producer
   before supporting arbitrary manual compatibility/native switching;
5. run concurrent speaker plus microphone, soak, permission revoke, focus,
   background/power and subjective-quality tests;
6. replace build-time peer authority and insecure UDP with paired Runtime
   authorization and the production transport/security policy.
