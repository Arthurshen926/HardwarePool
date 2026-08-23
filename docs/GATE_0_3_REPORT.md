# CapyIO Gates 0–3 Completion Report

> Date: 2026-08-23
>
> Task: CAPY-FOUNDATION-001
>
> Branch: `codex/capyio-foundation`

## Outcome

The verified HardwarePool bootstrap has been migrated to the CapyIO pre-alpha
foundation. The executable scope is still deterministic and hardware-free.

- Gate 0 retained the baseline at commit `d33d585` in
  `docs/BASELINE_REPORT.md`.
- Gate 1 migrated active names/packages to CapyIO and established PRD v0.3,
  architecture, ADRs, provenance and the Gates 0–15 roadmap.
- Gate 2 replaced device roles and audio Bindings with symmetric Nodes,
  Adapter-owned Capabilities, typed Ports and independent directed Routes.
- Gate 3 added the Adapter manifest/schema, bounded JSON-RPC/NDJSON SDK,
  process Host and finite Mock Source/Sink Sidecars.

## Executable evidence

The deterministic fixture connects these independent Routes:

```text
Phone Microphone -> Windows Virtual Microphone
Windows System Mix -> Phone Speaker
Phone IMU -> Windows Gamepad Projection
Phone Camera -> Camera Preview Panel
```

Both Nodes own Source and Sink Ports. Tests start all four Routes, stop one
without changing the other three and scope an Adapter process failure to only
the two Routes owned by the failed Windows audio Adapter.

The Adapter Smoke executable starts two real child processes and performs:

```text
initialize -> probe -> health -> catalog
prepare Route -> start -> status -> stop -> status
shutdown
```

It separately triggers exit code 23 and verifies that the Host reports the
unexpected exit while retaining stderr diagnostics. The one returned
`SmokeSample` is finite test data, not a media/sensor data plane.

## Validation executed

All commands below passed on the local Windows host:

```text
python scripts/validate_repository.py
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                       # 42 tests passed
cargo xtask validate-docs                    # 86 Requirement ID references
cargo xtask validate-manifests               # 2 manifests
cargo xtask adapter-smoke                    # source/sink/crash isolation
cargo xtask ci
corepack pnpm typecheck
corepack pnpm build
```

No new third-party production dependency was introduced; the new workspace
crates reuse the existing Serde/JSON/error dependencies.

## Main changed areas

- `crates/capyio-core`, `capyio-runtime`, `capyio-protocol`, `capyio-testkit`;
- `crates/capyio-adapter-sdk`, `crates/capyio-adapter-host`;
- `adapters/mock-source`, `adapters/mock-sink`;
- `apps/capyio-node`, `apps/desktop`;
- `protocol/proto/capyio/v1`, `protocol/schemas`;
- `xtask` and foundation documentation.

## Unresolved risks and explicit non-claims

- The Sidecar Host is sequential. It has bounded queues, correlations, retained
  diagnostics and a five-second response deadline, but no concurrent request or
  unsolicited-event architecture yet.
- Linux/macOS child-process behavior has not been exercised locally.
- The Protobuf contract is pre-alpha; golden/unknown-field compatibility
  fixtures are required before external consumers.
- Descriptor-count and metadata-string limits must be added before enabling an
  untrusted production transport.
- There is no Android project/APK, physical-device access, real audio/video/IMU
  path, Windows virtual endpoint/driver, pairing, encryption or WAN transport.
