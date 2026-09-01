# CAPY-PTP-002P — Admitted-channel delivery boundary

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`, `NFR-STAB-001..004`,
`NFR-SEC-001..003`, `NFR-PERF-001..003`, `NFR-MAINT-001..003`

## Objective

Define the fail-closed ownership and retry semantics between the private packet
Source and a future trusted-host authenticated transport without implementing or
claiming cryptography.

## In scope

- a host-supplied admitted-channel trait;
- exact Route, Session, Source, Sink, epoch and authorization-expiry binding;
- binding revalidation before construction, every send and normal close;
- distinct Delivered, RejectedBeforeWrite and DeliveryUnknown results;
- tentative Source encoding committed only after confirmed delivery;
- exactly-once channel cleanup on fault, close, construction failure and Drop;
- fixed-size typed binding mismatch diagnostics.

## Out of scope

- pairing, identity verification, TLS/Noise, keys or authenticated encryption;
- socket selection, framing, retry timers or reconnect;
- Kotlin/JNI, Android components, permissions, APK or physical devices;
- Windows device creation or desktop input.

## Acceptance criteria

1. A mismatched or unavailable channel cannot construct a delivery session.
2. Current admission and the complete binding are checked before each send.
3. Definite rejection before write leaves the exact frame retryable.
4. Unknown delivery, revoked admission or binding drift faults and closes.
5. A faulted/closed session cannot deliver more frames.
6. Channel cleanup occurs once on every owned terminal path.
7. Targeted tests, Clippy, full CI and repository validation pass.

## Required validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_delivery
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

## Completion evidence

- Six delivery-session tests pass.
- Tests distinguish safe same-frame retry from terminal delivery ambiguity.
- Construction failure, admission loss, binding drift, explicit close and Drop
  all retain bounded channel cleanup evidence.
- Targeted Clippy and full repository CI pass.
- No dependency, socket, cryptographic claim or device operation was added.

Detailed evidence: `docs/CAPY_PTP_002P_REPORT.md`.
