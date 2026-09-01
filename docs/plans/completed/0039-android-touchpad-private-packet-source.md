# CAPY-PTP-002O — Android capture to private packet Source

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`, `NFR-STAB-001..004`,
`NFR-SEC-001..003`, `NFR-PERF-001..003`, `NFR-MAINT-001..003`

## Objective

Connect the hardware-free Android capture session to a bounded Adapter-owned
packet Source without prematurely adding an unauthenticated network transport.

## Toolchain finding

The repository contains no Android Gradle project and the host exposes no
`java`, `gradle`, `kotlinc`, `adb`, `ANDROID_HOME` or `ANDROID_SDK_ROOT` through
the inspected environment. Adding Kotlin that cannot be compiled or tested is
out of scope for this slice.

## In scope

- an Adapter-owned Source around the existing private packet codec;
- mandatory initial `CancelAll` and contiguous sequence;
- transactional codec/sequence rejection and encoded-packet metrics;
- active-contact close refusal and terminal idempotent close;
- Android capture-to-encode/decode composition tests.

## Out of scope

- Kotlin/JNI, Gradle, Android components, permissions or APK work;
- socket I/O, peer authentication, encryption, retry or reconnect;
- Windows device creation or desktop input;
- physical-device operations.

## Acceptance criteria

1. No ordinary update can be encoded before the initial cancellation barrier.
2. Sequence gaps and invalid frames do not commit Source state or metrics.
3. One packet is produced per accepted frame within the existing 152-byte cap.
4. A Source cannot close with contacts retained and rejects input after close.
5. A real 002N multi-touch lifecycle round-trips through the private codec.
6. Targeted tests, Clippy, full CI and repository validation pass.

## Required validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_packet_source
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

## Completion evidence

- Three packet-Source tests pass.
- The composed test encodes and decodes seven Android capture frames including
  two-finger motion, complete release and final cancellation.
- Targeted Clippy and full repository CI pass.
- No dependency, socket, APK, permission or physical-device action was added.

Detailed evidence: `docs/CAPY_PTP_002O_REPORT.md`.
