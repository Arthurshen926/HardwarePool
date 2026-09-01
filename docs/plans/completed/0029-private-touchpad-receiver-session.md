# CAPY-PTP-002E — Private touchpad receiver session

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-007`, `FR-PLAT-004`,
`NFR-STAB-001..004`, `NFR-SEC-001..003`, `NFR-MAINT-001..003`

## Objective

Wrap private touchpad packet decoding in one fail-closed receiver lifecycle that
enforces bounded rate, replay/sequence checks, inactivity cleanup, disconnect
and explicit epoch transitions before frames can reach a Windows Sink.

## In scope

- a platform-neutral `PrivateTouchpadSink` lifecycle trait;
- a receiver bound to one packet codec, input sequence and configured Sink;
- fixed-window packet-rate limits and monotonic receive-time validation;
- duplicate/late rejection and observable sequence gaps;
- active-contact idle timeout that closes/cancels the Sink;
- explicit disconnect, epoch advance and abandoned-receiver cleanup;
- poison-on-packet, replay or Sink failure with retained cleanup errors;
- a Windows-only trait implementation for `SyntheticTouchpadSession`;
- deterministic fake/Windows-projector tests with no native device calls.

## Out of scope

- opening a real socket or choosing TCP/UDP/QUIC/WebSocket;
- authentication, encryption, peer discovery or pairing;
- public CapyDataPlane framing;
- Android runtime/JNI/Gradle/APK work;
- real desktop input or additional gesture acceptance;
- driver/VHF work.

## Acceptance criteria

1. Invalid receiver limits fail before accepting a packet.
2. Decode/rate/clock/replay faults close the Sink and poison the receiver.
3. Sink submission failure attempts close and retains both errors when needed.
4. Sequence gaps are observable and enter the Sink's existing cancel/suppress
   behavior; duplicate/late frames never reach it.
5. Active contacts time out at the configured bound, while an already released
   stream remains idle without spurious failure.
6. Epoch advance is strictly increasing, resets receive/rate state and advances
   the Sink before new packets are accepted.
7. Dropping an active receiver attempts bounded Sink close.
8. Ordinary tests perform no network or OS input operation.

## Required validation

```text
cargo fmt --all -- --check
cargo check -p capyio-remote-touchpad-adapter --all-targets
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo test -p capyio-remote-touchpad-adapter
cargo xtask validate-docs
cargo xtask ci
git diff --check
```

## Dependency changes

No external dependency. `capyio-windows-input` becomes a Windows-target local
dependency solely for the Sink trait implementation; cross-platform tests keep
using it as an existing workspace dev-dependency.

## Safety

- Receiver construction is not authorization; Runtime must authorize and bind
  the Route/peer before constructing it.
- Any structural/replay/rate/clock error closes the Sink rather than keeping
  possibly active contacts alive.
- Receive timestamps are supplied by a trusted local monotonic clock boundary,
  never by the packet sender.
- No command in this slice invokes user32 or opens a socket.

## Implementation plan

1. Define receiver limits, states, faults and Sink lifecycle.
2. Implement transactional receive, rate/replay checks and timeout cleanup.
3. Add explicit epoch/disconnect/Drop behavior and Windows Sink adaptation.
4. Add deterministic fault/lifecycle/projector tests.
5. Update docs, run full validation and archive this plan.

## Completion evidence

- `PrivateTouchpadReceiver` implements transactional decode/sequence/Sink
  submission, fixed-window admission, local monotonic-clock validation,
  active-contact timeout, disconnect, strict epoch advance and Drop cleanup.
- A Windows-only trait implementation adapts the existing
  `SyntheticTouchpadSession` without constructing a device.
- Eight deterministic receiver tests cover invalid bounds, replay, gaps,
  rate, clock, timeout, Sink/cleanup faults, epoch transition and close/drop.
- Focused checks and full `cargo xtask ci` pass; no socket, user32 device or
  desktop injection operation ran.

Detailed evidence: `docs/CAPY_PTP_002E_REPORT.md`.
