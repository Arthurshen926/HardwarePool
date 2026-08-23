# CapyIO Gates 0–3 Completion Report

> Date: 2026-08-23
>
> Task: CAPY-FOUNDATION-001
>
> Branch: `codex/capyio-foundation`

> Hardening addendum: 2026-08-24, `CAPY-FOUNDATION-002`. The original Gate
> evidence below remains historical; current hardening changes and validation
> results are recorded in `BUILD_STATUS.md`, `REQUIREMENTS_TRACEABILITY.md` and
> `FOUNDATION_HARDENING_REPORT.md`.

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
unexpected exit while retaining stderr diagnostics. After hardening, generic
Host methods return bounded Route acknowledgements/status rather than a
`SmokeSample`; the one finite test sample is a Mock-private response extension,
not a generic contract or media/sensor data plane.

## Validation executed

All commands below passed on the local Windows host:

```text
python scripts/validate_repository.py
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                       # 42 tests passed
cargo xtask validate-docs                    # historical formatting count: 86
cargo xtask validate-manifests               # 2 manifests
cargo xtask adapter-smoke                    # source/sink/crash isolation
cargo xtask ci
corepack pnpm typecheck
corepack pnpm build
```

No new third-party production dependency was introduced; the new workspace
crates reuse the existing Serde/JSON/error dependencies.

The old documentation command counted Markdown formatting tokens. The hardened
validator instead parses 84 unique normative Requirement IDs by pattern, rejects
duplicate/malformed definitions and checks complete status/Gate/evidence rows in
`REQUIREMENTS_TRACEABILITY.md`.

## 2026-08-24 hardening evidence delta

- Sidecar stdout is rejected at 64 KiB while reading; stderr retains a bounded
  2 KiB prefix/marker and drains the oversized line.
- Timeout, unexpected ID, malformed/oversized response and stdout closure place
  the sequential Host in `Poisoned`, close stdin, terminate/reap the child and
  reject future requests. Late response 1 cannot become response 2.
- Route control uses generic bounded prepare/start/stop/status DTOs. Continuous
  payloads remain outside JSON-RPC.
- Manifest v2 expresses deployment-mode-specific bindings. Runtime catalog and
  Route-backend tests cover scoped invalidation/recovery and unsupported modes.
- Exact-head hosted workflows now define the three-OS Rust/Adapter gates,
  frozen-lockfile UI gate and Windows Tauri gate. A hosted result is still
  pending and must not be inferred from workflow source.

## Main changed areas

- `crates/capyio-core`, `capyio-runtime`, `capyio-protocol`, `capyio-testkit`;
- `crates/capyio-adapter-sdk`, `crates/capyio-adapter-host`;
- `adapters/mock-source`, `adapters/mock-sink`;
- `apps/capyio-node`, `apps/desktop`;
- `protocol/proto/capyio/v1`, `protocol/schemas`;
- `xtask` and foundation documentation.

## Unresolved risks and explicit non-claims

- The Sidecar Host remains sequential and has no concurrent response
  demultiplexer or unsolicited-event architecture. Protocol desynchronization
  is deliberately terminal and requires Supervisor restart.
- Linux/macOS child-process behavior has not been exercised locally on this
  Windows host; configured hosted jobs are evidence only after they run.
- The Protobuf contract is pre-alpha; golden/unknown-field compatibility
  fixtures are required before external consumers.
- Descriptor-count and metadata-string limits must be added before enabling an
  untrusted production transport.
- There is no Android project/APK, physical-device access, real audio/video/IMU
  path, Windows virtual endpoint/driver, pairing, encryption or WAN transport.
