# CAPY-PTP-002P Report

Date: 2026-08-30

Status: admitted-channel delivery and retry-ambiguity boundary complete;
hardware-free validation passes.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

`PrivateTouchpadDeliverySession` now owns the transition from the transactional
packet Source to a trusted-host-supplied admitted channel. The channel must
reassert the complete Runtime binding—Route, Session, Source, Sink, epoch and
authorization expiry—at construction and before every send.

The session clones the Source into tentative state before encoding. Confirmed
delivery commits that state. A channel guarantee that no write occurred leaves
the same frame retryable. An unknown result is terminal because retrying could
duplicate a click, scroll or system gesture; the session closes the channel and
faults before accepting further input.

## Fail-closed behavior

- unavailable, expired or revoked admission faults before a write attempt;
- any individual binding-field drift faults before a write attempt;
- DeliveryUnknown faults after exactly one attempt and prohibits retry;
- active-contact Source close failure leaves the channel owned and usable for a
  release/cancellation frame;
- construction failure, explicit close, fault and abandoned Drop close the
  owned channel exactly once;
- mismatch errors retain only the differing fixed-size typed fields rather than
  allocating or copying two complete bindings.

## Truthful security boundary

`PrivateTouchpadAdmittedChannel` is an interface implemented only by fake test
channels in this slice. It allows a future trusted host to supply authenticated
state, but it does not authenticate a peer, encrypt data, manage keys, open a
socket or provide network replay protection. Real authenticated transport
remains required.

## Validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_delivery
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
```

Results: six targeted tests passed, Clippy passed with warnings denied, and full
repository CI passed.

## Files

- `adapters/remote-touchpad/src/delivery.rs`
- `adapters/remote-touchpad/src/source.rs`
- `adapters/remote-touchpad/src/lib.rs`
- `adapters/remote-touchpad/tests/touchpad_delivery.rs`
- `adapters/remote-touchpad/README.md`
- `docs/plans/completed/0040-touchpad-admitted-channel-delivery-boundary.md`
- architecture, protocol, security, testing, product scope, build status and
  traceability documentation.

## Remaining work

- Pairing identity, authenticated encryption, key/version binding and real
  replay protection remain absent.
- A concrete bounded transport and Runtime-owned admission provider remain
  absent.
- Android OS/JNI and APK work still require a buildable Android toolchain.
- No network, Android device or Windows input operation was performed.
- The worktree remains uncommitted and based on `fc3da36`.
