# CAPY-TOUCHPAD-001A Report

Date: 2026-08-29

Status: minimal portable conversion slice and focused validation complete.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

This slice replaces the reserved remote-touchpad marker with a deterministic,
hardware-free `touch-events/1` complete-snapshot to `pointer-events/1`
converter. It changes only `adapters/remote-touchpad/**` and this report.

The converter:

- treats Touch input and Pointer output as distinct streams with independent
  IDs, epochs and sequence spaces;
- requires both stream descriptors to name the same source clock because the
  converter preserves source timestamps rather than relabeling them;
- anchors the first single contact without moving the pointer;
- maps later single-contact deltas one-for-one from normalized-coordinate
  least-significant bits to semantic relative pointer counts;
- maps a stationary release inside 250 ms and 512 coordinate units to primary
  press/release followed by Reset;
- enters primary-button drag after a stationary 500 ms hold, maps subsequent
  deltas while held, and emits release followed by Reset on the empty snapshot;
- emits Reset for empty snapshots, sequence gaps, explicit epoch changes,
  lifecycle cleanup, contact-ID replacement, timestamp regression and
  unsupported multi-contact snapshots;
- suppresses non-empty snapshots after a gap or ambiguous contact set until an
  empty snapshot restores a known state;
- emits no more than two Pointer frames and two events per frame for one input
  snapshot; and
- applies changes transactionally, so validation or output-sequence failure
  does not partially advance converter state.

`reset()` is the explicit lifecycle fail-safe for Route stop/offline, Adapter
failure or peer loss. `advance_epoch()` advances both stream epochs and emits a
Reset in the fresh Pointer epoch before later input is accepted.

## Tests

Eight deterministic tests cover:

- single-contact relative movement plus empty-snapshot Reset;
- short-tap primary click plus Reset;
- long-hold drag press, movement, release and Reset;
- a sequence gap while a drag is held, suppression and recovery after empty;
- explicit input/output epoch advance and fresh output sequence;
- multi-contact Reset without gesture expansion;
- rejection of shared input/output identity and clock-domain relabeling; and
- transactional failure when a two-frame tap cannot fit before output sequence
  exhaustion.

## Validation evidence

The following commands completed successfully in the touchpad worktree:

```text
cargo fmt --manifest-path adapters/remote-touchpad/Cargo.toml -- --check
cargo check -p capyio-remote-touchpad-adapter
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo test -p capyio-remote-touchpad-adapter
cargo xtask check
git diff --check
```

The focused test run passed all 8 integration tests plus library and doctest
targets. `cargo xtask check` additionally passed workspace check and Clippy
with warnings denied.

No root Workspace file, `Cargo.lock`, public Profile contract, shared roadmap,
driver, platform host or other worktree was changed. No network, hardware,
driver, APK or operating-system input API was used.

## Boundaries and unresolved risks

- This is semantic conversion only. It does not implement Windows `SendInput`,
  HID, VIIPER, a driver, transport, Android capture or UI.
- Two-finger scroll, right click, double click, pinch, three/four-finger
  gestures and Precision Touchpad reports are intentionally absent.
- A long press becomes a drag only when an active-contact snapshot is observed
  at or after the 500 ms threshold. There is no timer or background worker in
  this pure slice.
- Motion sensitivity, acceleration, DPI and multi-monitor selection remain
  Projection policy. The current one-to-one normalized-unit mapping is chosen
  for deterministic fixture evidence, not as a final physical feel claim.
- Public `TouchFrame`/`PointerFrame` payloads use bounded `Vec` fields. The
  converter's retained state is fixed-size and every emitted collection has a
  fixed semantic maximum, but this slice does not claim real-time callback
  allocation suitability.
- Input authorization, replay defense, transport framing and Route lifecycle
  orchestration remain Runtime/data-plane responsibilities.

## Approval boundary

No commit, push or pull request was performed.
