# CAPY-PTP-002N — Android touchpad runtime capture boundary

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `NFR-STAB-001..004`, `NFR-SEC-001..003`,
`NFR-PERF-001..003`, `NFR-MAINT-001..003`

## Objective

Add the bounded, hardware-free state boundary that a future Android
`MotionEvent` callback can drive before Kotlin/JNI and APK integration.

## In scope

- explicit Stopped, Running and Closed capture states;
- fixed storage for at most five active Android pointer IDs;
- cross-event validation for down, pointer-down, move, pointer-up, up and cancel;
- start, stop/restart and close cancellation frames;
- transactional state on lifecycle, mapping and timestamp errors;
- deterministic pure Rust tests.

## Out of scope

- Kotlin/JNI, Gradle, Android Activity/View or Compose code;
- manifest, permission, foreground service, notification or APK changes;
- physical-device installation or testing;
- network transport, pairing or production Runtime scheduling.

## Acceptance criteria

1. Motion is rejected outside the Running state.
2. Pointer identities cannot appear, disappear or change outside their matching
   Android lifecycle action.
3. Failed events consume neither mapping sequence nor active-pointer state.
4. Start, stop and active close emit `CancelAll`; stop and close are idempotent.
5. No growing active-pointer collection or platform side effect is introduced.
6. Targeted tests, Clippy, full CI and repository validation pass.

## Required validation

```text
cargo test -p capyio-android-host
cargo clippy -p capyio-android-host --all-targets -- -D warnings
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

## Completion evidence

- Four capture-session tests and four existing mapping tests pass.
- Targeted Clippy passes with warnings denied.
- Full repository CI and post-document validation pass.
- No Android component, permission, APK, socket or physical-device operation was
  created or executed.

Detailed evidence: `docs/CAPY_PTP_002N_REPORT.md`.
