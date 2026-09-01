# CAPY-PTP-001 — Direction-neutral Precision Touchpad Profile

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-CAP-001..006`, `FR-PORT-002..005`,
`FR-PROTO-001..006`, `NFR-STAB-001..004`, `NFR-MAINT-001..003`

## Objective

Define and fixture-test one bounded five-contact touchpad Profile that preserves
the raw semantics needed by Android and Windows sources and by synthetic/VHF
Windows projections without depending on a platform SDK or driver ABI.

## Read first

- `AGENTS.md`
- the eight normative documents listed by it;
- `crates/capyio-core/AGENTS.md`;
- `crates/capyio-protocol/AGENTS.md`;
- ADR 0041 and ADR 0042.

## In scope

- canonical `capyio.input.touchpad-frames/1` Profile and
  `touchpad-frame-v1` format helpers;
- `Touchpad` Core/wire Capability class using append-only enum value 16;
- validated descriptor, frame, contact, button, metrics and fixture types;
- 3..=5 contacts, himetric coordinates, confidence, optional size/pressure;
- complete snapshots, explicit cancel-all and gap/epoch suppression;
- a deterministic committed fixture and hardware-free tests;
- normative Profile, Protocol and Testing documentation.

## Out of scope

- Android `MotionEvent`, permissions, UI or APK operations;
- Windows pointer/contact injection or gesture execution;
- network framing, pairing or production transport;
- VHF/HID descriptor, driver build/install/signing/deployment;
- source-side physical touchpad capture or exclusive suppression;
- modification of the existing generic touch Profile or pointer fallback.

## Architecture constraints

- `capyio-core` contains only the Profile/Capability identifiers;
- `capyio-input` stays safe, deterministic, platform/transport independent;
- Protobuf carries the appended catalog enum only, never touchpad frames;
- all lists and fixture sizes are bounded;
- invalid/gapped epochs cannot leave a Projection free to apply stale contact
  state;
- serde is a fixture/diagnostic shape, not a wire or native ABI.

## Acceptance criteria

1. Generic touch and touchpad Profile IDs remain distinct and validated.
2. Descriptor validation enforces himetric size, 3..=5 contacts and closed
   button/capability semantics.
3. Frames reject duplicate/overflowing contacts, out-of-range coordinates,
   undeclared optional data and malformed cancel-all state.
4. Tracker reports gaps and suppresses later updates until cancel-all; epoch
   advance also requires cancel-all before updates resume.
5. Canonical fixture replays deterministically through one through five
   contacts and ends released/cancelled.
6. `Touchpad` catalog values round-trip through Protobuf value 16.
7. Focused and available repository checks pass.

## Required tests and evidence

```text
cargo fmt --all -- --check
cargo check -p capyio-core -p capyio-input -p capyio-protocol --all-targets
cargo clippy -p capyio-core -p capyio-input -p capyio-protocol --all-targets -- -D warnings
cargo test -p capyio-core -p capyio-input -p capyio-protocol
cargo xtask validate-docs
cargo xtask ci
```

## Dependency changes

None.

## Safety and approvals

- privileged/device operations required: no;
- forbidden operations: input injection, driver/APK/device mutation,
  commit, merge/rebase, push and pull request.

## Implementation plan

1. Record ADR 0042 and append the Core/Profile identifiers.
2. Implement validated touchpad types and fail-safe tracker.
3. Add the deterministic fixture and boundary/round-trip tests.
4. Update normative docs and retain the exact validation report.

## Risks

- The new Profile lives on an unmerged input-contract worktree and must later
  be reconciled with current `main`.
- A portable Profile cannot itself prove Windows gesture behavior; that remains
  `CAPY-PTP-002` physical acceptance.
- Microsoft marks parts of the new user-mode API pre-release, but the Profile
  deliberately does not encode that API's native layout.

## Completion record

Implemented:

- distinct touchpad Profile/format and appended Core/Protobuf Capability;
- bounded descriptor, contact, frame, tracker, metrics and fixture types;
- allocation-free duplicate-ID validation over the five-contact bound;
- explicit initial/gap/epoch cancellation barriers;
- deterministic one-to-five-contact fixture and focused contract tests;
- ADR 0042 and all affected normative documentation.

Validation:

- focused format/check/Clippy/tests: pass;
- Protobuf value 16 round-trip: pass;
- `cargo xtask validate-docs`: pass;
- full `cargo xtask ci`: pass.

Not validated:

- Android capture, Windows frame injection or physical multi-finger gestures;
- production transport or reconnect behavior;
- VHF driver behavior.

Detailed evidence: `docs/CAPY_PTP_001_REPORT.md`.
