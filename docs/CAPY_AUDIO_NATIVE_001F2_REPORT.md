# CAPY-AUDIO-NATIVE-001F2 physical acceptance report

Date: 2026-08-31

Status: service-owned native microphone physically accepted with an ordinary
Windows recording client on the controlled lab pair

## Accepted composition

The exact deployed Release consists of:

- `capyio-windows-service.exe` SHA-256
  `73A44A596A2D9722FF0CE1A8F2D48564151238FEB7B3A1375D473ED9D84309AF`;
- `capyio-native-virtual-speaker.exe` SHA-256
  `1E26D822269DC4882664EE50BAB95A06CEC9F097C569D4F3C7F3ED9FAD2E97AC`;
- `capyio-native-virtual-microphone.exe` SHA-256
  `2A826EC4FA4C4F996FC0F15BD6705FE65DC4E134D18387304BCB375F778001AD`.

`CapyIOBroker` runs as an automatic LocalSystem service. Native speaker uses
Windows/phone UDP 46001/46000 and native microphone uses 46011/46010. The
service creates the capture ring and supervises both children; no MicYou process
or MicYou Android application participates.

The final accepted Android diagnostic build is `0.4.2-dev`, version code 10,
SHA-256
`79DE1A0BA7C6CDDE08D33A3C15D2C971027B7EC181EC6034DC7F6B83B5A68578`.
It corrects stale compatibility-era copy, exposes bounded microphone sender
metrics without retaining/logging audio content and coalesces worker-driven UI
notifications to at most one refresh per 250 ms.

## Ordinary-client recording evidence

An ordinary CPAL/WASAPI capture client opened the CapyIO virtual microphone at
48 kHz mono float32 and wrote eight-second S16 WAV files. Three relevant runs
produced:

1. 384,000 samples, 374,080 non-zero, RMS 0.00190556, peak 0.05099487;
2. after Broker Runtime Stop/Start, 384,000 samples, 372,649 non-zero,
   RMS 0.00078120, peak 0.01446533;
3. after installing 0.4.1-dev, 384,000 samples, 378,387 non-zero,
   RMS 0.00365161, peak 0.17431641;
4. on the final 0.4.2-dev build, 384,000 samples, 382,496 non-zero,
   RMS 0.05668539, peak 0.52511597.

Every client exited zero with no stderr. Each WAV was 768,044 bytes. Raw WAV
and machine-local scripts remain ignored under `.agent-cache/` and `target/`;
no captured audio is committed.

## Lifecycle and direction evidence

- generation 1 reached `active` with both service children and retained the
  honest `receiverPresent=false` UDP semantics;
- Runtime Stop returned `stopped` and released both UDP 46001 and 46011;
- Runtime Start advanced to generation 2, reached `active` and assigned new
  process owners to both ports;
- the second non-silent WAV proved capture recovery after that restart;
- Android reported foreground service types `0x82`, proving microphone and
  media-playback capabilities were simultaneously owned;
- the 0.4.1-dev panel observed 1,002,240 microphone frames, 2,088 packets
  generated, 2,088 packets/UDP datagrams sent, zero queue drops and zero
  buffered bytes while the speaker Route remained independently active.
- the final 0.4.2-dev UI exposed exact clickable bounds under active metrics;
  stopping microphone reduced foreground ownership from `0x82` to `0x02`, and
  stopping speaker removed the foreground service. A later microphone-only
  final-WAV run also stopped cleanly. Earlier missed ADB taps were coordinate
  errors after active text reflow, not an application stop failure.

## Gate conclusion

`CAPY-AUDIO-NATIVE-001F` is functionally accepted for the controlled local-LAN
lab. It proves a normal Windows application can record live Android microphone
PCM through the CapyIO Android application, common audio packet, native backend,
service-owned receiver, capture ring and virtual endpoint without MicYou.

This does not qualify production security, WAN use, independent Desktop Route
controls, codec/AEC, clock correction, long soak, permission revoke, device
route changes, signed installation or release distribution. Native and MicYou
capture producers remain mutually exclusive.
