# ADR 0046: Connect the native speaker path through bounded user-mode workers

- Status: accepted for `CAPY-AUDIO-NATIVE-001E`
- Date: 2026-08-31

## Context

ADR 0045 ends at a complete Android receive queue. It does not write PCM to
`AudioTrack`, and the Windows virtual-speaker Broker still sends only the Audio
Share private protocol. The first physical native switchover must preserve the
already tested virtual endpoint and render APO while keeping network authority
outside the Activity, audio callback and Windows driver.

## Decision

1. Move the protocol-neutral `Global\CapyIO.RenderRing.v1` consumer into the
   internal `capyio-windows-render-ring` platform crate. The Audio Share Broker
   reuses that crate unchanged; this is code ownership extraction, not removal
   of the compatibility baseline.
2. Add one `capyio-native-virtual-speaker` user-mode Broker. It owns the render
   ring, converts the existing float32 blocks to S16LE, packetizes exact 480-
   frame/10 ms stereo packets and sends them through ADR 0044. Network and
   packet logic remain outside the driver and APO.
3. Add a deterministic `capyio-native-audio-tone` sender so native transport
   and Android render can be qualified before involving the Windows endpoint.
4. Android reads one closed trusted-lab configuration from build-time Manifest
   metadata. The default build disables it; peer IPv4 and ports enter only
   through bounded build environment values. The Activity exposes no IP, port,
   UUID or epoch input.
5. The controlled lab fixes 48 kHz, stereo, S16LE, 10 ms and a dedicated
   Session/Route/Stream/epoch. These identifiers are test authority, not
   production pairing or authenticated control binding.
6. `NativeLanPcmSinkWorker` drains complete packets on a background worker,
   validates payload geometry, handles partial non-blocking writes, detects
   sequence/sample gaps and stops within two seconds. A gap or packet
   discontinuity resets the streaming sink before the next packet.
7. `SpeakerSinkAdapter` owns `AudioTrack`, the UDP receiver and PCM sink worker
   under one generation. It uses `AudioTrack.WRITE_NON_BLOCKING`; neither the
   Activity nor an Android audio callback performs network I/O.
8. Physical acceptance remains separate: update exactly the controlled debug
   package, require visible user start, prove tone and ordinary Windows-client
   audio, then test Stop, peer loss and fresh start. Until that evidence exists,
   001E is implementation-complete but not functionally accepted.

## Controlled-lab result

The 0.3.4-dev build completed the native tone baseline and ordinary Windows
`SoundPlayer` path through the working CapyIO endpoint with zero Android
transport, reassembly or queue drops. Windows retained a stale identically
named endpoint instance that did not feed the current ring, so stable instance
selection and cleanup remain release requirements. Phone Stop/Start produced a
fresh zeroed generation and a second application playback succeeded without
drops. The native Broker was subsequently placed behind the existing
`CapyIOBroker` service control boundary. It reached `active` without falsely
claiming UDP receiver presence, delivered two ordinary Windows WAV runs to the
phone, released UDP 46001 on service stop and reacquired it after service
restart. In-flight sender-loss recovery, abrupt-termination/installer upgrade,
soak and subjective audio-quality checks remain outside this acceptance result.

## Consequences

- The native speaker path no longer needs the Audio Share Android application
  or private wire, while the compatibility executable remains available for
  comparison and rollback.
- The Windows render-ring mechanism is now reusable without importing Audio
  Share transport behavior into the native backend.
- The lab path is intentionally insecure and fixed-configured. Manifest values
  do not authenticate the peer; ADR 0044 security limitations still apply.
- There is no jitter deadline, late-packet policy, resampling, clock correction,
  concealment or codec. A discontinuity flushes buffered output and may be
  audible.
- Production configuration must later come from the Runtime's paired Route
  authority rather than build metadata.
