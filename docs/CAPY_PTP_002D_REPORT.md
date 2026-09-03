# CAPY-PTP-002D Report

Date: 2026-08-30

Status: private packet bridge complete; focused and full repository validation
pass.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The Android and Windows touchpad platform boundaries now share a deterministic,
bounded private packet representation without opening a network connection or
injecting operating-system input. The codec lives in the remote-touchpad
Adapter as an `AdapterManaged` contract; Core, Protobuf/JSON-RPC,
`capyio-input` and the transport-independent `capyio-data-plane` remain free of
a public wire-layout commitment.

## Packet boundary

`PrivateTouchpadPacketCodecV1` is constructed for one validated
`InputStreamDescriptor` and `TouchpadDescriptor`. It preserves:

- stream epoch, frame sequence and source monotonic timestamp;
- update versus cancel-all lifecycle;
- integrated button state;
- zero to five stable contact IDs and himetric positions;
- confidence; and
- optional contact size and normalized pressure.

StreamId and clock domain are established by the surrounding Route/session and
are not repeated per frame. The exact packet length is:

```text
32 + 24 * contact_count
```

The maximum packet is 152 bytes. Encoding uses a fixed-capacity array. Decode
allocates only the contract's bounded zero-to-five contact vector.

## Fail-closed behavior

Encode rejects invalid stream setup, wrong StreamId, stale/future epochs and
semantic frame violations. Decode rejects:

- packets below 32 or above 152 bytes;
- bad magic or unknown versions;
- invalid frame/button values;
- contact counts beyond the negotiated descriptor;
- any non-exact length;
- unknown contact flags and non-zero reserved bytes;
- absent size/pressure fields with non-zero backing data; and
- all ordinary semantic violations such as duplicate IDs, coordinates outside
  the declared surface and invalid cancel-all contents.

Epoch advancement is explicit and strictly increasing. This codec deliberately
does not authenticate, authorize, encrypt, open sockets, enforce a replay
window, schedule packet rates or reconnect a peer. A future transport must
establish those properties before calling decode.

## Android-to-Windows boundary loop

The hardware-free integration test composes:

```text
Android MotionEvent DTO mapper
  -> TouchpadFrame
  -> private packet encode/decode
  -> WindowsTouchpadProjector
  -> bounded active/released contact batches
```

It covers initial cancellation, one-finger down, second-finger down, reordered
two-contact movement, one-contact release and final release. IDs, scaled
himetric positions and pressure survive the packet, while the Windows projector
produces the expected sorted active/release records.

No `user32` device or injection API is called by these tests.

## Files

- `adapters/remote-touchpad/src/wire.rs`
- `adapters/remote-touchpad/src/lib.rs`
- `adapters/remote-touchpad/Cargo.toml`
- `adapters/remote-touchpad/tests/touchpad_packet.rs`
- `adapters/remote-touchpad/tests/touchpad_boundary_loop.rs`
- `adapters/remote-touchpad/README.md`
- `docs/adr/0044-keep-touchpad-packet-framing-private-to-adapter.md`
- `docs/REMOTE_TOUCHPAD_PRIVATE_PROTOCOL.md`
- `docs/plans/completed/0028-private-touchpad-packet-bridge.md`
- architecture, data-plane, protocol, security, testing, Port Profile and
  traceability documentation.

## Dependency note

No external production dependency was added. The existing Android and Windows
workspace platform crates are dev-dependencies of the remote-touchpad Adapter
only so the cross-platform boundary loop can compile and run without hardware.

## Validation

The following commands pass:

```text
cargo fmt --all -- --check
cargo check -p capyio-remote-touchpad-adapter --all-targets
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo test -p capyio-remote-touchpad-adapter
cargo xtask validate-docs
cargo xtask ci
```

The Adapter test run contains eight existing generic touch-to-pointer fallback
tests, six private-packet tests and one Android-to-Windows multi-contact
boundary-loop test. Full CI passed workspace formatting, check, Clippy, tests,
demo, documentation, manifests, Adapter smoke, repository validation and
frontend typecheck/build.

## Remaining work and risks

- No live transport exists; the packet is not safe to expose directly to an
  unauthenticated network.
- Stream/Route authentication, authorization, encryption, replay/rate limits,
  disconnect and reconnect policy remain required.
- Android runtime/JNI touch capture is still DTO-only.
- Windows production Runtime wiring to `SyntheticTouchpadSession` remains
  separate from the lab harness.
- Visible two/three/four-finger behavior still requires separately approved
  physical acceptance.
- The private packet can be replaced by a future public CapyDataPlane binding;
  it is not an interoperability promise.
- The worktree remains uncommitted and based on `fc3da36`.
