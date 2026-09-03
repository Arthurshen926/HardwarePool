# CAPY-GAMEPAD-001A Report

Date: 2026-08-29

Status: transport-free implementation and automated validation complete

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-gamepad`

Branch: `codex/capyio-gamepad`

## Outcome

This slice implements a deterministic DSU protocol projection without opening
a network socket or touching hardware. It adds:

- bounded parsing of the three official DSU v1001 client request shapes;
- table-free CRC-32/ISO-HDLC verification and response checksums;
- fixed-size version, port-info and 100-byte pad-data response encoders;
- one available logical controller at slot 0; port-info reports slots 1 through
  3 as disconnected and pad-data generation rejects them as unavailable;
- validation of canonical `DataEnvelope<ImuSampleV1>` input;
- explicit, validated signed axis permutations;
- acceleration conversion from m/s² to standard gravity and angular velocity
  conversion from rad/s to deg/s;
- deterministic motion timestamps derived from the acceleration component
  timestamp when present, otherwise the source timestamp;
- explicit neutral `capyio-input::GamepadControls` output. Non-neutral controls
  are rejected because their DSU button mapping is not part of this slice.

The parser accepts only bounded datagrams. It follows DSU's declared-length
rule by rejecting short datagrams and ignoring bytes beyond the declared
logical packet. Unofficial motor and rumble messages are rejected.

## Public-contract boundary

The Adapter consumes `DataEnvelope<ImuSampleV1>` from `capyio-data-plane` and
uses the canonical `ImuSampleV1::profile()` helper rather than copying the
profile string. That helper returns the public IMU profile
`capyio.motion.imu-samples/1`. DSU motion remains an Adapter projection; it is
not merged into the `capyio.input.gamepad-state/1` contract.

The packet encoder accepts `capyio-input::GamepadControls`, validates it and,
for this slice, requires `GamepadControls::neutral()`. Route-level stream,
epoch, sequence, queue and gap policy stays in the shared data plane rather
than being reimplemented by the DSU codec.

Axis mapping is a required, explicit projection boundary. The identity mapping
used by tests is only a deterministic fixture choice and does not assert the
correct orientation for every phone mounting or emulator.

## Files

- `adapters/dsu/src/crc32.rs`
- `adapters/dsu/src/motion.rs`
- `adapters/dsu/src/protocol.rs`
- `adapters/dsu/src/lib.rs`
- `adapters/dsu/tests/imu_projection.rs`
- `adapters/dsu/README.md`
- `docs/CAPY_GAMEPAD_001A_REPORT.md`

No root manifest, lockfile, public crate or global normative document was
changed. No production dependency was added.

## Automated evidence

The crate tests cover:

- the standard CRC-32 check value;
- valid version, port-info and pad-data requests;
- connected port-info only for slot 0, disconnected port-info for slots 1
  through 3, and explicit unavailable errors for their pad-data;
- declared-length truncation, CRC corruption, unsupported message types,
  invalid counts, flags and slots, short packets, invalid magic/version,
  declared-length underflow and the hard datagram-size bound;
- selector validation that applies slot bounds only when slot-based matching is
  enabled, without silently reinterpreting all-controller or MAC-only requests;
- application of explicit signed axis permutations and rejection of duplicate
  source-axis mappings;
- all six records in `fixtures/imu/imu_samples_v1.jsonl` projected to
  deterministic fixed-size DSU packets with neutral controls;
- DSU header, length, little-endian fields, CRC, timestamp and unit conversion;
- acceleration-component timestamp precedence;
- rejection of invalid/non-neutral controls and finite f64 values that overflow
  f32.

Commands run successfully in this worktree:

```text
cargo test --locked -p capyio-dsu-adapter
cargo check --locked -p capyio-dsu-adapter
cargo clippy --locked -p capyio-dsu-adapter --all-targets -- -D warnings
cargo fmt --package capyio-dsu-adapter -- --check
cargo xtask validate-docs
cargo xtask validate-manifests
cargo xtask ci
git diff --check
```

## Architecture and third-party records

No ADR is required for this slice: ADR 0040 already reserves DSU as an
Adapter-owned projection and keeps gamepad, IMU and haptics semantics separate.

The existing `third_party/THIRD_PARTY.yml` record
`dsu-protocol-reference` pins
`https://github.com/v1993/cemuhook-protocol` revision
`82bf8a837cc7d2254e9257729f462a233d9ad184`, protocol version 1001 and the
Unlicense. `source_imported: false` and `binary_imported: false` remain correct:
this slice implements the public protocol description but imports no upstream
source or binary. The central integration owner may update the record's review
note from future tense; this feature worktree deliberately does not edit the
shared provenance file.

## Remaining risks and next gate

- No emulator interoperability is claimed; the evidence is codec- and
  fixture-level only.
- A real endpoint needs a separately reviewed bounded subscriber registry,
  endpoint policy, renewal timeout, rate limit, lifecycle and security tests.
- A physical mounting needs an explicit and measured axis/sign mapping.
- Button, stick, trigger and touch projection is intentionally absent. The
  current encoder fails closed for non-neutral gamepad controls.
- Packet scheduling and DSU packet-number ownership belong to the future
  transport/lifecycle slice.
- Adding controller slots 1 through 3 requires an explicit inventory/lifecycle
  decision; this slice does not advertise them as connected.
- A real phone, APK, Android permission, driver, fixed port and production
  network service were not used or modified.

No commit or push was performed.
