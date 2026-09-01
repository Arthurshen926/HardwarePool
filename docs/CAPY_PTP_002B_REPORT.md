# CAPY-PTP-002B Report

Date: 2026-08-30

Status: controlled injection harness complete; desktop injection and physical
gesture acceptance not executed.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The Windows platform crate can now own a generic synthetic Precision Touchpad
for the lifetime of a controlled process and submit the bounded native batches
implemented by `CAPY-PTP-002A`. A separate CLI provides four fixed one-shot
gesture fixtures, but defaults to dry-run and rejects injection unless an
explicit mode and a separate desktop-impact acknowledgement are both present.

No input batch was submitted while implementing or validating this slice.

## Device and failure lifecycle

`SyntheticTouchpadDevice`:

- validates `1..=5` contacts and non-zero himetric dimensions before loading a
  platform module;
- loads only System32 `user32.dll`;
- dynamically resolves `CreateSyntheticPointerDevice2`,
  `InjectSyntheticPointerInput` and `DestroySyntheticPointerDevice`;
- retains the module for the entire synthetic-device lifetime;
- turns missing exports, load/create failure and submission failure into typed
  errors;
- skips empty batches rather than calling Windows with a zero count; and
- destroys a non-null handle exactly once before unloading the module.

The CLI attempts to project and submit a cancelled cleanup batch if a fixture
submission fails after native contacts became active. Device destruction remains
the final cleanup path even if cancellation also fails.

## Fixed fixtures

The closed fixture catalog contains:

- `one-finger-motion`;
- `two-finger-pan`;
- `three-finger-swipe`; and
- `four-finger-swipe`.

Every fixture uses a 100 x 60 mm non-clickable surface, one stream epoch, stable
contact IDs, an initial `cancel_all`, eight 15 ms active updates, an empty
release snapshot and a final `cancel_all`. User-provided coordinates, timing,
steps and repeat counts are not accepted.

Dry-run projected each fixture into 11 frames and nine native batches:

```text
gesture              contact records  peak contacts  device creation  injected
one-finger-motion     9                1              not requested    false
two-finger-pan        18               2              not requested    false
three-finger-swipe    27               3              not requested    false
four-finger-swipe     36               4              not requested    false
```

## CLI safety behavior

The ordinary command is dry-run:

```text
cargo run -p capyio-windows-input --bin capyio-ptp-inject -- --gesture one-finger-motion --dry-run
```

Native submission requires both:

```text
--inject --acknowledge-desktop-input
```

Using `--inject` without the acknowledgement fails during argument parsing,
before fixture projection, user32 loading or device creation. Conflicting modes,
acknowledgement in dry-run mode, missing gestures, unknown gestures and unknown
arguments are also rejected.

These flags reduce accidental execution; they are not production authorization,
authentication or a network security boundary.

## Validation

These commands completed successfully without input submission:

```text
cargo fmt --all -- --check
cargo check -p capyio-windows-input --all-targets
cargo clippy -p capyio-windows-input --all-targets -- -D warnings
cargo test -p capyio-windows-input
cargo run -p capyio-windows-input --bin capyio-ptp-inject -- --gesture one-finger-motion --dry-run
cargo run -p capyio-windows-input --bin capyio-ptp-inject -- --gesture two-finger-pan --dry-run
cargo run -p capyio-windows-input --bin capyio-ptp-inject -- --gesture three-finger-swipe --dry-run
cargo run -p capyio-windows-input --bin capyio-ptp-inject -- --gesture four-finger-swipe --dry-run
cargo xtask validate-docs
cargo xtask ci
git diff --check
```

The crate passed four API/device unit tests, three CLI parser tests, three fixed-
fixture tests and five platform projection tests. Full repository CI passed
formatting, workspace check/Clippy/tests, fixture demo, documentation,
manifests, Adapter smoke, structural validation and frontend typecheck/build.

## Files

- `platform/windows/capyio-input/src/lib.rs`
- `platform/windows/capyio-input/src/injection_fixture.rs`
- `platform/windows/capyio-input/src/bin/capyio-ptp-inject.rs`
- `platform/windows/capyio-input/tests/injection_fixture.rs`
- `platform/windows/capyio-input/README.md`
- `docs/plans/completed/0026-controlled-touchpad-injection-harness.md`
- `docs/ARCHITECTURE.md`
- `docs/SECURITY_MODEL.md`
- `docs/TESTING.md`
- `docs/REQUIREMENTS_TRACEABILITY.md`

## Dependency note

No dependency or feature was added in this slice.

## Remaining evidence and risks

The harness has not yet established that Windows:

- accepts any encoded PT_TOUCHPAD frame on this host;
- moves the pointer for the one-finger fixture;
- pans/scrolls for the two-finger fixture; or
- recognizes configured three/four-finger system gestures.

It also does not prove tap-to-click, integrated buttons, Android/network input,
Windows Settings behavior, RDP/multi-user behavior, sleep/resume, crash cleanup
or VHF compatibility. Microsoft's current API remains marked pre-release.

The next action is a controlled run of the exact one-finger command, followed by
observation and retained output. Two-, three- and four-finger commands should be
approved separately because they can scroll content or change the visible
desktop/application state.

## Integration risk

The worktree remains based on `fc3da36`, while public `main` was observed at
`d4df224`; a reviewed integration operation is still required. Preserved
uncommitted generic touch-to-pointer work remains in the same worktree.

No commit, merge, rebase, push or pull request was performed.
