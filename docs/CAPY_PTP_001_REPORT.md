# CAPY-PTP-001 Report

Date: 2026-08-30

Status: direction-neutral touchpad contract slice complete; platform projection
and physical gesture acceptance pending.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

CapyIO now has a platform-independent, bounded Precision Touchpad contract. It
preserves the multi-contact state needed by Android sources, Windows physical
touchpad sources and Windows synthetic/VHF projections without putting a
Windows, Android, HID or transport ABI in `capyio-core` or `capyio-input`.

The slice adds:

- the distinct `capyio.input.touchpad-frames/1` Profile and
  `touchpad-frame-v1` diagnostic format;
- a `Touchpad` Capability class, appended as Protobuf enum value 16;
- a validated himetric physical-surface descriptor with 3..=5 contacts and
  click-pad, pressure-pad or non-clickable button declarations;
- complete active-contact snapshots containing stable contact IDs,
  coordinates, confidence and optional contact size/pressure;
- explicit `cancel_all` frames and fail-safe suppression of initial,
  post-gap and post-epoch updates until cancellation is observed;
- bounded deterministic fixture replay and exact metrics; and
- ADR 0042 plus normative architecture, domain, Profile, protocol, testing and
  requirement-traceability updates.

The existing generic `capyio.input.touch-events/1` Profile is unchanged. No
touchpad frame was injected and no driver, APK, device permission, network
transport or physical input device was modified.

## Contract decisions

- Each ordinary frame is a complete active-contact snapshot. A missing
  contact is released by the platform Projection using its last accepted
  state.
- `cancel_all` must contain no contacts and a released integrated button.
- A fresh stream, sequence gap or epoch change places the tracker behind a
  cancellation barrier; ordinary updates remain suppressed until
  `cancel_all` arrives.
- Physical coordinates and dimensions use himetric units (1/100 mm), not
  display pixels.
- Frames remain on the high-rate data path. Protocol v1 only exposes the
  appended Capability catalog value and does not serialize touchpad frames in
  the Protobuf control envelope.
- Serde JSON is a bounded fixture/diagnostic representation, not a production
  wire or native ABI.

## Fixture evidence

`fixtures/input/touchpad_frames_v1.json` contains eight frames and exercises
one, two, four and five simultaneous contacts between initial and final
`cancel_all` frames. Validation reports exactly:

```text
frames_observed=8
contact_samples_observed=13
peak_contacts=5
sequence_gaps=0
cancel_all_frames=2
suppressed_frames=0
```

The focused contract suite has eight tests covering Profile separation,
descriptor bounds, duplicate/overflow/out-of-surface contacts, undeclared
optional fields, non-clickable button state, malformed cancellation,
gap/epoch cancellation barriers, transactional timestamp rejection, fixture
round-trip/tail rules, unknown fields and stream identity.

The protocol test also proves that `Touchpad` maps to appended wire value 16
and round-trips back to the Core class.

## Validation

These commands completed successfully:

```text
cargo fmt --all -- --check
cargo check -p capyio-core -p capyio-input -p capyio-protocol --all-targets
cargo clippy -p capyio-core -p capyio-input -p capyio-protocol --all-targets -- -D warnings
cargo test -p capyio-core -p capyio-input -p capyio-protocol
cargo test -p capyio-input --test touchpad_contract
cargo test -p capyio-protocol --test roundtrip touchpad_capability_uses_appended_wire_value_16
cargo xtask validate-docs
cargo xtask ci
```

Full repository CI passed Rust format/check/Clippy/tests, fixture demo,
documentation, manifests, Adapter smoke, structural validation and frontend
typecheck/build.

## Files

- `crates/capyio-core/src/capability.rs`
- `crates/capyio-core/tests/profile_registry.rs`
- `crates/capyio-input/src/error.rs`
- `crates/capyio-input/src/formats.rs`
- `crates/capyio-input/src/lib.rs`
- `crates/capyio-input/src/touchpad.rs`
- `crates/capyio-input/tests/touchpad_contract.rs`
- `crates/capyio-protocol/src/conversion.rs`
- `crates/capyio-protocol/tests/roundtrip.rs`
- `protocol/proto/capyio/v1/common.proto`
- `fixtures/input/touchpad_frames_v1.json`
- `fixtures/input/README.md`
- `docs/adr/0042-separate-touchpad-frames-from-generic-touch-snapshots.md`
- `docs/plans/completed/0024-direction-neutral-touchpad-profile.md`
- `docs/PRODUCT_REQUIREMENTS.md`
- `docs/ARCHITECTURE.md`
- `docs/DOMAIN_MODEL.md`
- `docs/INPUT_PROFILE.md`
- `docs/PORT_PROFILES.md`
- `docs/PROTOCOL.md`
- `docs/REQUIREMENTS_TRACEABILITY.md`
- `docs/TESTING.md`

## Dependency note

No dependency was added. `capyio-input` uses its existing Serde and Core
dependencies; Protobuf evolution appends one enum value without changing any
existing number or field.

## Unresolved evidence and risks

This contract does not yet prove:

- Android `MotionEvent` capture, contact-ID mapping or cancellation behavior;
- Windows contact projection, pointer motion/click or two-finger pan/zoom;
- Windows-native three/four-finger system gestures;
- Windows Settings visibility, natural-scroll configuration, RDP, multi-user,
  sleep/resume or process-crash cleanup;
- production transport, pairing, Route lifetime or reconnect cancellation;
- VHF/HID enumeration, certification, signing or driver compatibility; or
- physical Windows touchpad Mirror/Exclusive source behavior.

`FR-SCEN-006` therefore remains `planned` for Gate 10 even though the
`CAPY-PTP-000` and `CAPY-PTP-001` foundation slices are complete. The next
small slice is `CAPY-PTP-002`: implement platform boundary mappings and a
controlled one-to-five-contact Windows Projection harness, then retain
physical one/two/three/four-finger acceptance evidence.

## Integration risk

This worktree is still based on `fc3da36`, while public `main` was observed at
`d4df224`; the two lines must be reconciled in a separately reviewed
integration operation. The worktree also contains preserved, uncommitted
generic touch-to-pointer fallback changes that predate this slice.

No commit, merge, rebase, push or pull request was performed.
