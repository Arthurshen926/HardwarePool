# ADR 1556: Finite camera admission in the Android Rust sender

Status: experimental sender implementation; hosted acceptance required. This is
not an Android Camera2 finite-capture implementation or device compatibility claim.

## Scope and compatibility

The actual Rust-owned `SecureCameraSender` recognizes the exact parameter-free
`cavc-avc-finite-720p30-v1` format introduced by ADR 1555. Legacy remains a separate
profile with its existing behavior. There is no implicit upgrade, fallback,
format inference from duration, new wire protocol, crypto or resource authority.

Android's production capability catalog still advertises only
`cavc-avc-access-unit-v1`. The Java capture gate explicitly refuses a finite or
unknown native capture format. Existing Camera2 selection, sensor rotation and
MediaCodec behavior cannot yet promise the finite canonical profile. Adding a
Rust sender is not permission to relabel those frames or start a camera.

## Independent sender budgets

Finite preparation binds the selected profile, TLS Route/Session/Stream identity,
original Source lease deadline and a monotonic lifetime anchor. Negotiation and
handshake consume that original window. Repeated equivalent offers retain the
first preparation; they do not reconstruct admission or extend its deadline.
Changed profile, endpoint or original authority is rejected. The existing durable
per-Route generation claim remains one-shot after failure, Stop and worker reaping.

The sender independently enforces the same deterministic local profile ceilings
as the Linux receiver, without depending on Linux or native decoder crates:

- At most ceil(remaining milliseconds × 30 / 1000) non-EOS AUs, capped at 1,800.
- Complete CAVC input bytes, capped at min(32 MiB, 2 MiB/s × remaining time + 4 MiB).
- Local-monotonic arrival buckets of 30 AUs/s with three-AU burst and 2 MiB/s with
  four-MiB burst. PTS, discontinuity and repeated status cannot reset them.
- Exact declared 1280×720, 30 fps, zero rotation, Annex B config, with bounded CSD,
  record ordering, stream/epoch identity, sequence and timestamp checks.

Work-count rounding never extends time authority. The receiver has its own budget
from its remaining lifetime; numeric work limits are not newly exchanged over the
wire. Each sender write still checks original authority before and after bounded
TLS I/O. Any admission, timing, transport or identity failure closes the sender.
No retry, reconnection, renewal or automatic new generation is introduced.

Declared-config validation is not a complete independent H.264 decoder or proof
of sensor orientation. The native Linux receiver retains its strict SPS/PPS,
slice, B-frame and actual decoded-output gates. Full profile-aware Camera2 and
MediaCodec validation remains a separate prerequisite before advertisement.

## Memory and lifecycle

The existing two-record reservation bound includes reserved Java copies, queued
records and the worker's current record. Reservation occurs before JNI copying;
parsing remains length-bounded. The sender does not collect a complete stream.
Permission loss, current-profile mutation, Stop, control EOF, revoke and expiry
retain the existing one-shot cancellation and reconciliation paths.

`secureConfig` identifies the exact immutable format. This does not authorize Java
capture: the current Java path accepts only the legacy format. Existing local
permission and independent counterpart consent remain required; TLS readiness
or encoded/sent counters do not synthesize receiver Active or delivery evidence.

## Evidence and remaining work

Pure tests cover finite arithmetic, rate/work exhaustion, canonical configuration,
replay and non-resetting generation/preparation behavior. Existing hosted CI builds
ARM64 JNI and compiles actual Android Java using preinstalled pinned tools, and
runs hardware-free Java contracts. No SDK installation, APK signing/deployment,
real credential creation, user keyring access or physical capture is performed.

A separate mandatory hosted fixture invokes the production `SecureCameraSender`
with synthetic encoded frames and an isolated pinned peer/protected store, then
uses the actual Linux Node and independent native PipeWire pixel consumer. Nine
profiles cover sustained transmission, local and sender Stop, control Stop/EOF,
Source revoke, exact expiry and sender-side rate/byte rejection. The sustained
case submits 300 paced AUs; actual consumer count/span and late pixels are checked
independently, not inferred from sender or queue counters. Sender-side malicious
input rejection is not claimed as bytes reaching the receiver. The previous
receiver-side malicious-input scenarios remain separate and mandatory.

Next work must explicitly gate a canonical Camera2 size/FPS/rotation policy,
encoder capability/output validation and opt-in advertisement. Until then an
existing Android phone remains legacy-only, regardless of this sender proof.

## Subsequent bounded capture slice

ADR 1557 adds explicit experimental Java capture eligibility/output gates. The
legacy-only statements above describe this ADR’s original sender-only slice; the
initial option was restricted to eligible orientation-zero cameras. ADR 1558
subsequently adds an actual cardinal pixel-normalization stage. Neither is a
physical-phone compatibility claim.
