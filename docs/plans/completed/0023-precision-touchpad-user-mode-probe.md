# CAPY-PTP-000 — Windows synthetic Precision Touchpad API probe

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-PLAT-004`, `NFR-SEC-003`,
`NFR-STAB-001..004`, `NFR-MAINT-001..003`

## Objective

Determine whether the identified Windows host can create a user-mode synthetic
Precision Touchpad through the new runtime API without installing a driver or
injecting any contact input.

## Read first

- `AGENTS.md`
- `docs/PRODUCT_REQUIREMENTS.md`
- `docs/ARCHITECTURE.md`
- `docs/DOMAIN_MODEL.md`
- `docs/ADAPTER_MODEL.md`
- `docs/PORT_PROFILES.md`
- `docs/PROTOCOL.md`
- `docs/SECURITY_MODEL.md`
- `docs/TESTING.md`
- `platform/windows/AGENTS.md`
- ADR 0041

## In scope

- runtime discovery of the four required System32 `user32.dll` exports;
- deterministic validation of `1..=5` contacts and non-zero physical himetric
  dimensions;
- an explicit create-and-immediately-destroy smoke with no contact injection;
- hardware-free unit tests and retained local-host evidence;
- truthful unavailable/failure outcomes on older Windows versions.

## Out of scope

- `TouchpadFrame`, Android `MotionEvent`, networking or Route integration;
- contact/gesture injection and cursor movement;
- 1/2/3/4-finger, Windows Settings, RDP or multi-user acceptance;
- VHF/HID driver source, build, install, signing or deployment;
- APK installation, Android permissions or foreground services;
- source-side physical touchpad capture or exclusive suppression.

## Architecture constraints

- Windows FFI remains inside `platform/windows/capyio-input`;
- `capyio-core` and `capyio-input` remain safe, deterministic and
  platform-independent;
- runtime symbol absence is a supported result, not a panic or compatibility
  claim;
- successful device handles are destroyed on the same bounded call path;
- the probe never loads a DLL from the working/current directory.

## Acceptance criteria

1. The symbol-only probe reports every required export and injects no input.
2. Invalid contact count/physical size fails before a platform call.
3. The explicit local smoke either records a typed Windows error or creates and
   destroys the synthetic touchpad without injecting a frame.
4. Tests, formatting, Clippy and repository validation available to this
   worktree pass.
5. Evidence states that gesture behavior and end-to-end Android sharing remain
   unverified.

## Required tests and evidence

```text
cargo fmt --manifest-path platform/windows/capyio-input/Cargo.toml -- --check
cargo check -p capyio-windows-input --all-targets
cargo clippy -p capyio-windows-input --all-targets -- -D warnings
cargo test -p capyio-windows-input
cargo run -p capyio-windows-input --bin capyio-ptp-probe -- --symbols-only
cargo run -p capyio-windows-input --bin capyio-ptp-probe -- --create-device
cargo xtask validate-docs
```

Artifacts to retain:

- target OS build and installed SDK version;
- stable probe output without pointer addresses;
- commands and result summary in `docs/CAPY_PTP_000_REPORT.md`.

## Dependency changes

The platform crate enables `Win32_Foundation` and
`Win32_System_LibraryLoader` on the already pinned workspace
`windows-sys 0.61.2` dependency. No new package or network download is added.
The library is MIT/Apache-2.0 and already used by the repository's Windows
platform code. Direct Windows FFI without generated primitive bindings was the
alternative; the pinned binding reduces declaration mistakes while the new
pre-release functions themselves remain dynamically resolved.

## Safety and approvals

- privileged/device operations required: no;
- exact target for retained evidence: current Windows development host;
- forbidden operations: driver build/install/remove, boot/security changes,
  gesture injection, APK/device mutation, commit, push and pull request.

## Implementation plan

1. Record ADR 0041 and narrow the normative non-goal exception.
2. Implement the portable result model and Windows runtime loader.
3. Add a default symbol-only CLI plus explicit create-device smoke.
4. Run focused validation and retain exact local evidence.
5. Start `CAPY-PTP-001` only after this feasibility result is reviewed.

## Risks

- Microsoft still marks the API pre-release and may change behavior/signatures.
- An exported function can still reject creation or behave differently under
  RDP, another user session or a later Windows build.
- Create/destroy success does not prove physical-touchpad equivalence or
  Windows Settings integration.

## Completion record

Implemented:

- runtime System32 `user32.dll` discovery for all four required exports;
- validated five-contact himetric creation parameters;
- a symbol-only CLI that performs no device creation or input injection;
- an explicit create/device destroy smoke with no submitted contact frames;
- unit tests, ADR 0041 and normative scope/testing updates.

Validation:

- focused format/check/Clippy/test: pass;
- symbol-only probe: all four exports present on the identified host;
- explicit device smoke: five-contact 100 x 60 mm device created and destroyed;
- `cargo xtask validate-docs`: pass;
- full `cargo xtask ci`: pass.

Not validated:

- all gesture and Android end-to-end acceptance listed above.

Follow-up issues:

- `CAPY-PTP-001`: direction-neutral five-contact touchpad Profile and fixtures;
- `CAPY-PTP-002`: Android touch surface to Windows synthetic Projection;
- `CAPY-PTP-003`: separately approved VHF compatibility fallback;
- `CAPY-PTP-004/005`: Windows physical touchpad Mirror/Exclusive sources.

Detailed evidence: `docs/CAPY_PTP_000_REPORT.md`.
