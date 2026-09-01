# CapyIO native audio LAN lab backend

This crate is the first executable transport behind the direction-neutral
`capyio-audio` media seam. It moves one already-negotiated
`AudioMediaPacket` over an explicit-peer UDP socket on a worker thread.

It is intentionally a lab reference, not a production or StandardPort wire:

- backend ID: `dev.capyio.audio.lan-lab/1`;
- interoperability: `AdapterManaged`;
- media access: full common packet, PCM or already-encoded Opus payload;
- datagram bound: 1,200 bytes, including a fixed 104-byte header;
- packet bound: 64 fragments / 70,144 payload bytes;
- reassembly bound: 1–8 in-flight packets;
- peer: one preselected literal unicast IP and non-zero UDP port;
- socket deadline: 1–2,000 ms;
- authentication, encryption, integrity, replay and downgrade protection: all
  absent.

The wire header carries Session, Route, Stream, epoch, sequence, source time,
first sample index, sample count, discontinuity and canonical fragmentation
metadata in network byte order. The selected audio format remains in the
control-approved binding and is not repeated in each datagram.

`fixtures/audio/native_lan_v1_opus_single.hex` is a shared Rust/Java golden
datagram. Rust loopback tests additionally cover fragmentation, reverse-order
reassembly, duplicates, bounded eviction, malformed data, wrong bindings,
wrong peers and deadlines.

Platform audio callbacks must never call this crate's socket methods. ADR 0046
connects the Android speaker through separate fixed-capacity receiver/render
workers. Until authenticated transport and authorization binding exist, use
this only on an explicitly trusted local lab network.

The 001E lab provides two bounded senders:

```text
cargo run -p capyio-native-audio-lan --bin capyio-native-audio-tone -- <windows-ip:46001> <phone-ip:46000> [seconds]
cargo run -p capyio-native-audio-lan --bin capyio-native-virtual-speaker -- <windows-ip:46001> <phone-ip:46000> [seconds]
```

The tone isolates wire/Android playback. The Windows-only virtual-speaker
Broker owns `Global\CapyIO.RenderRing.v1`, packetizes its S16LE conversion and
uses the same lab binding. Both remain insecure controlled-lab tools.
