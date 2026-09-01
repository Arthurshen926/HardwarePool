# ADR 0044: Establish a bounded native-audio LAN reference backend before transport selection

- Status: accepted for `CAPY-AUDIO-NATIVE-001D1`
- Date: 2026-08-31

## Context

ADRs 0041–0043 provide one direction-neutral media binding, truthful
compatibility-backend contracts and a CapyIO-owned Android audio service. The
two physically proven paths still use Audio Share and MicYou private protocols,
while the native path has no executable data plane. Selecting a large media
stack before CapyIO has a measurable transport conformance fixture would make
packet identity, bounds and failure behavior dependent on that stack.

AOO is a relevant later candidate: its official project offers peer-to-peer
Sources/Sinks, PCM and Opus, jitter/reorder/loss/retransmission handling,
resampling and dynamic clock adjustment under an Improved BSD license. Its
official README also identifies the project as alpha and warns that breaking
changes can occur between pre-releases. SonoBus is a GPL-3.0 application built
on AOO and is useful as an architectural/reference baseline, but importing the
application does not provide a narrow CapyIO transport boundary. Neither
project is imported by this slice.

## Decision

Implement a small CapyIO-authored UDP reference backend first, behind the
existing `AudioTransportBackendContract`:

1. identify it as `dev.capyio.audio.lan-lab/1`, `AdapterManaged`, full-packet,
   PCM/Opus-capable and exact for the common packet metadata;
2. bind every endpoint in advance to one Session, directed Route, Stream,
   positive epoch and exact selected `AudioStreamSpec`; the datagram has no
   microphone/speaker or Source/Sink role bit;
3. accept only one explicit unicast IP/port peer and bounded 1–2,000 ms socket
   deadlines; discovery, DNS, negotiation and reconnect are outside this wire;
4. use a fixed 104-byte, big-endian version-1 header:

| Offset | Bytes | Field |
|---:|---:|---|
| 0 | 4 | magic `CPYA` |
| 4 | 1 | wire version `1` |
| 5 | 1 | flags; bit 0 is discontinuity |
| 6 | 2 | canonical header length `104` |
| 8 | 16 | Session UUID |
| 24 | 16 | Route UUID |
| 40 | 16 | Stream UUID |
| 56 | 4 | stream epoch |
| 60 | 8 | packet sequence |
| 68 | 8 | source timestamp, microseconds |
| 76 | 8 | first sample index |
| 84 | 4 | sample count |
| 88 | 4 | complete packet payload bytes |
| 92 | 4 | fragment offset |
| 96 | 2 | fragment index |
| 98 | 2 | fragment count |
| 100 | 2 | fragment payload bytes |
| 102 | 2 | reserved zero |

5. cap a UDP datagram at 1,200 bytes, one fragment payload at 1,096 bytes, one
   packet at 64 fragments/70,144 payload bytes and reassembly at 1–8 concurrent
   partial packets; reject non-canonical offsets/counts and conflicting data;
6. retain unsigned 64-bit counter bit patterns exactly on Java, where a signed
   `long` is only the storage container and callers use unsigned operations;
7. execute sockets and reassembly only on bounded media workers. Android audio
   callbacks continue to perform no network operation, file I/O or ordinary
   logging;
8. add Android `INTERNET` permission for this approved native-transport slice,
   but keep the Java endpoint outside `AudioRecord`/`AudioTrack` callbacks and
   do not claim remote sound until the queue integration is physically tested;
9. declare peer authentication, confidentiality, integrity, replay protection
   and downgrade binding false. This backend is limited to an explicitly
   trusted local/Tailscale lab and is not a production StandardPort.

The Rust implementation uses only existing workspace dependencies. The Android
mirror uses Java/Android platform APIs and adds no APK runtime dependency. One
shared golden datagram fixes the Rust/Java byte layout.

## Consequences

- CapyIO now has a direction-neutral, replaceable and measurable native media
  transport reference independent of Audio Share and MicYou.
- The reference proves wire identity, bounded fragmentation/reassembly,
  malformed/wrong-peer rejection and local UDP movement; it does not yet feed
  Android capture/render or either Windows virtual endpoint.
- There is no jitter deadline, late-loss policy, retransmission, congestion
  control, clock correction, resampler, codec implementation, pairing or
  cryptography. Those omissions are explicit rather than inferred as parity.
- `CAPY-AUDIO-NATIVE-001D2` must connect fixed-capacity platform/transport
  queues and prove synthetic cross-device movement before 001E/001F switch
  speaker and microphone product paths.
- A future AOO spike must remain behind the same backend contract, pin upstream
  revision/license/imported paths and compare measured latency, loss recovery,
  clock behavior and operational complexity against this fixture.
