# CAPY-AUDIO-CORE-001 unified audio media contracts report

Status: local implementation and authorized physical Speaker regression complete

Date: 2026-08-28

## Outcome

`capyio-audio` now provides one transport- and platform-independent contract for
independent microphone and speaker Routes. It adds complete selected stream
candidates, PCM/Opus identity without codec implementation, three initial QoS
presets, explicit processing requests, bounded deterministic negotiation and
direction-neutral metrics.

ADR 0035 records that this common media layer does not merge microphone and
speaker Capability, Port, Route, permission or lifecycle state. Concrete
capture/render APIs, codecs, transports, processing engines and Windows
projections remain Adapter/platform responsibilities.

The Audio Share compatibility sender now accepts the common `MediaBalanced`
PCM specification and maps its observable counters to the common metrics
snapshot. Its pinned protobuf format bytes remain unchanged.

## Automated evidence

The following commands passed on the Windows development host:

```text
cargo test -p capyio-audio -p capyio-audio-share-adapter
cargo xtask validate-docs
cargo xtask ci
cargo xtask adapter-smoke
```

The full workspace run covered 15 `capyio-audio` unit tests and two audio
integration tests. Audio Share covered its bounded transport, pinned wire,
render-ring and supervisor tests; tests that require a separately supplied
external executable remain explicitly ignored.

One full-run failure exposed an existing Windows port-zero race: UDP could
select an ephemeral port excluded from TCP. The compatibility transport now
retries the dual-protocol ephemeral selection at most 32 times. An explicit
operator-selected port still receives one exact attempt and its original error.

Documentation validation passed with 88 unique Requirement IDs traced. Desktop
typecheck/build and Adapter smoke also passed.

## Authorized Android regression

The existing, already installed Audio Share v0.3.4 receiver was connected over
the authorized wireless ADB target. No APK, permission, driver or Windows audio
package was installed or changed.

The repository-built tone sender used the common `MediaBalanced` specification
and delivered a bounded 10-second, 48 kHz stereo PCM stream:

```text
blocks_enqueued=1000
queue_full=0
blocks_without_receiver=0
datagrams_sent=2000
datagram_send_errors=0
pcm_bytes_sent=1920000
```

Android created the expected 48 kHz stereo `AudioTrack`. The test intentionally
makes no subjective audible-quality claim because the human operator was not
beside the phone. After the isolated run, the receiver was restored to the
existing service port and `CapyIOBroker` returned to `active` with receiver
presence true.

## Deferred work and risks

- `CAPY-MIC-000` must pin, build and audit MicYou before source or protocol
  reuse; its GPL-3.0 boundary is not resolved by this Gate.
- Opus is only a semantic encoding identifier. No codec, packetization, PLC,
  FEC or interoperability is implemented or claimed.
- Negotiation currently uses exact complete-candidate intersection. Resampling,
  transcoding and processing changes require a future explicit Converter.
- The QoS presets are bounded policy defaults, not measured latency or quality
  claims.
- High-resolution and multichannel playback remain separate physical
  experiments.

