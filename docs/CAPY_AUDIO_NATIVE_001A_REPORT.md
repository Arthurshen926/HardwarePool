# CAPY-AUDIO-NATIVE-001A — Backend-neutral audio media seam report

Date: 2026-08-31

Status: complete locally

## Outcome

CapyIO now has the missing common seam between a platform audio engine and a
replaceable concrete transport. One `AudioMediaStreamBinding` fixes the typed
Session, directed Route, Stream, positive epoch and exact selected
`AudioStreamSpec`. The binding intentionally contains no microphone/Speaker or
Source/Sink role.

`AudioMediaPacket` carries the shared sequence, monotonic source timestamp,
sample position/count, discontinuity and payload contract. PCM converts to and
from the existing decoded `AudioFrame` without changing any field or byte.
Encoded payload is representable and bounded without importing or claiming an
Opus implementation.

`BoundedAudioPacketQueue` is a deterministic worker-thread reference boundary.
It rejects wrong Stream/epoch data, invalid packet payloads, packet-count
overflow and aggregate-byte overflow explicitly. It never silently evicts an
accepted packet and is explicitly not the platform callback ring.

## Changed implementation

- `crates/capyio-audio/src/media.rs` — binding, media packet, public limits,
  bounded queue, counters and unit tests;
- `crates/capyio-audio/src/error.rs` — typed binding/packet/queue failures;
- `crates/capyio-audio/src/lib.rs` — public media contract exports;
- `crates/capyio-audio/tests/audio_pipeline.rs` — opposite-route isolation
  integration test;
- ADR 0041 and active plan 0022 — compatibility/native migration boundary;
- architecture, Profile, protocol, security, testing, backlog, roadmap,
  status and traceability documentation.

No production dependency or third-party source was added. Audio Share and
MicYou code, private wire bytes, Windows drivers/APOs/rings and Android packages
were not modified.

## Automated evidence

- `cargo test -p capyio-audio`: PASS — 24 passed, 0 failed;
- `cargo clippy -p capyio-audio --all-targets -- -D warnings`: PASS;
- `cargo xtask validate-docs`: PASS — 88 unique Requirement IDs;
- `cargo xtask doctor`: required bootstrap tools/files present; optional ADB,
  MSBuild and WinDbg were not visible in this shell and were not required;
- `cargo xtask ci`: PASS — format, workspace check, Clippy, 165 passed Rust
  tests with 4 explicit ignored external/physical tests, deterministic IMU demo,
  repository/docs validation, 2 manifests, Adapter smoke, desktop typecheck and
  Vite build.
- `cargo xtask demo`: PASS — deterministic symmetric Node/Session/four-Route
  snapshot still builds and runs.

No socket, phone, APK, driver, Windows service or media recording was used by
this evidence.

## Compatibility statement

The new packet is a semantic in-process value, not a public network
serialization. ADR 0006 remains in force: framing, MTU/fragmentation,
authentication, encryption, replay windows, downgrade binding and transport
candidate selection are still separate decisions.

Audio Share and MicYou remain independent `AdapterManaged` compatibility
contracts. Sharing `AudioMediaStreamBinding` does not claim that either private
wire preserves all CapyIO metadata or interoperates with the other.

## Unresolved risks and next slice

`CAPY-AUDIO-NATIVE-001B` should put explicit compatibility-backend adapters on
the new seam and document which metadata each private wire preserves or loses,
with pinned-wire regression tests. Only after that boundary is stable should a
CapyIO Android service/permission slice and an AOO/native transport spike begin.

The following remain unproven: network interoperability, latency/jitter under
real transport, codec quality, authenticated peer identity, Android
capture/playback lifecycle, simultaneous physical duplex behavior, 96 kHz or
multichannel playback, DSP/AEC and removal of either external application.
