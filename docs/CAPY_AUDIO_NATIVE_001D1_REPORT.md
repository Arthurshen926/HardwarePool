# CAPY-AUDIO-NATIVE-001D1 implementation report

Date: 2026-08-31

Status: local transport-conformance Gate complete; platform-audio and physical
cross-device integration not started

## Outcome

CapyIO now owns a first executable, direction-neutral audio transport reference
behind the ADR 0041 media seam. The same packet and backend contract can carry a
microphone Source Route or speaker Sink Route; neither the wire nor endpoint
contains a device-role flag. The existing Audio Share and MicYou paths remain
unchanged physical regression baselines.

This slice does not yet produce remote sound. It stops at bounded Rust/Java UDP
codec and worker endpoints. Android microphone bytes are still discarded and
the speaker `AudioTrack` remains empty until `001D2` connects fixed-capacity
platform/transport queues.

## Implemented transport boundary

- backend ID `dev.capyio.audio.lan-lab/1`;
- `AdapterManaged`, full common-packet access, PCM/already-encoded Opus support
  and exact common metadata fidelity;
- fixed big-endian version-1 `CPYA` header containing Session, Route, Stream,
  epoch, sequence, source time, sample index/count, discontinuity and canonical
  fragment metadata;
- 104-byte header and 1,200-byte datagram ceiling;
- at most 1,096 bytes per fragment, 64 fragments and 70,144 packet payload
  bytes;
- 1–8 incomplete packets per Rust reassembler;
- one preselected explicit unicast IP/non-zero port and 1–2,000 ms socket
  deadline;
- explicit duplicate, wrong-peer, wrong-binding, malformed, conflict, timeout
  and partial-eviction observations;
- a 120-byte golden datagram consumed by both Rust and Java tests;
- exact unsigned 64-bit wire-counter preservation in Java signed-`long` bit
  containers.

## Android boundary

The approved `INTERNET` permission is now present and the development package
version is `0.2.0-dev`/code 2. `NativeLanPacketCodec` and
`NativeLanUdpEndpoint` compile into the application from a dependency-free Java
source boundary. The endpoint uses fixed datagram buffers and must run on a
media worker. Existing `AudioNodeService`, `MicrophoneSourceAdapter` and
`SpeakerSinkAdapter` contain no Java networking API, so this change does not
move a socket into an Android real-time callback.

The app still disables backup and cleartext application traffic, exports no
audio service and has no configured peer or automatic network start. Raw UDP
has no TLS-like protection; the cleartext manifest setting must not be read as
transport encryption.

## Dependency and candidate record

The Rust crate uses only the already reviewed workspace `capyio-audio`,
`capyio-core`, `thiserror` and `uuid` dependencies. Android uses platform Java
network APIs. No production dependency, third-party source or binary was added.

ADR 0044 records AOO as the strongest current follow-up candidate because its
official project exposes peer Sources/Sinks, PCM/Opus, jitter/loss handling,
retransmission, resampling and clock adjustment under an Improved BSD license.
The same official project describes itself as alpha and allows breaking changes
between pre-releases, so this slice does not make it the native default. SonoBus
is retained only as a GPL-3.0 application/reference built on AOO; none of its
code is imported.

## Automated evidence

- `cargo clippy -p capyio-native-audio-lan --all-targets -- -D warnings`:
  PASS;
- `cargo test -p capyio-native-audio-lan`: PASS — 8 tests;
- `cargo xtask android-check`: PASS;
  - 36 Android audio lifecycle assertions;
  - 35 native-LAN codec/golden-wire/UDP assertions;
  - Android Java/resources/manifest compilation;
  - Lint with warnings denied;
  - debug APK assembly;
- `python scripts/validate_repository.py`: PASS — 88 Requirement IDs and the
  static Android callback/transport separation plus wire-bound checks;
- `cargo xtask ci`: PASS — format, workspace check, warnings-as-errors Clippy,
  181 passed Rust tests plus 4 explicitly ignored external/physical tests, IMU
  demo, documentation/manifests, Adapter smoke, repository validation and
  desktop typecheck/build;
- `git diff --check`: PASS.

## Security truth

The backend contract sets peer authentication, confidentiality, integrity,
replay protection and downgrade binding to false. A configured UDP source
address and matching self-declared Route identifiers are filtering and
consistency checks, not identity or authorization. This backend is limited to
an explicitly trusted local/Tailscale lab and is neither production transport
nor a public StandardPort.

## Unresolved risks and next slice

- connect platform callbacks to fixed-capacity queues without blocking,
  unbounded allocation or socket work in the callback;
- compose one native Windows endpoint with one Android worker and prove
  synthetic cross-device packets, disconnect and fresh-epoch recovery;
- add reorder/jitter deadline and explicit late/loss behavior before audible
  physical switching;
- measure latency, loss, clock drift and buffer pressure against the current
  Audio Share/MicYou baselines;
- decide whether to implement resampling/retransmission directly or spike a
  pinned AOO Adapter behind the same contract;
- add pairing, Capability/Route authorization, authenticated encryption,
  replay windows and downgrade binding before any production claim;
- only then perform separately authorized APK/device sound tests and the
  speaker/microphone switchover slices `001E/001F`.
