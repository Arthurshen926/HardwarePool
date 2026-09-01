# CAPY-PTP-002T — Pre-authenticated stream records

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`, `NFR-STAB-001..004`,
`NFR-SEC-001..003`, `NFR-PERF-001..003`, `NFR-MAINT-001..003`

## Objective

Define and implement exact bounded Hello/Data/Ack/Close records for the private
touchpad path before introducing an authenticated Android-to-Windows stream.

## In scope

- 24-byte fixed record header with magic, version, kind, flags, epoch and
  sequence;
- 160-byte Hello binding complete Runtime Route identity and authorization
  expiry;
- Data wrapping one private packet with a 176-byte maximum;
- exact Ack and Close records;
- outer/embedded epoch and sequence agreement;
- canonical fields and fail-closed malformed-record handling;
- ADR 0045 security and acknowledgement-ambiguity rules.

## Out of scope

- socket I/O, connection scheduling, discovery or reconnect;
- pairing, keys, authenticated encryption or replay persistence;
- Android JNI/APK or device connection;
- Windows native input beyond the separately approved 002S evidence.

## Acceptance criteria

1. Every record size and field is exact and bounded.
2. Hello mutation or Route binding mismatch is rejected.
3. Data outer and embedded epoch/sequence must agree.
4. Unknown version/kind/flags/reserved data and malformed lengths fail closed.
5. Ack is exact; missing/mismatched Ack cannot authorize same-epoch retry.
6. Targeted tests, Clippy, full CI and repository validation pass.

## Required validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_transport_record
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

## Completion evidence

- Four transport-record tests pass.
- Maximum five-contact Data is exactly 176 bytes.
- Full-binding Hello, exact Ack/Close and malformed-record paths are covered.
- Full repository CI and documentation validation pass.
- No dependency, socket, key, APK or device operation was added.

Detailed evidence: `docs/CAPY_PTP_002T_REPORT.md`.
