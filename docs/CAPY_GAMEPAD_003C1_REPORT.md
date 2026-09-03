# CAPY-GAMEPAD-003C1 report

Date: 2026-08-29

Branch: `codex/capyio-gamepad`

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

## Result

The VIIPER Adapter now has a bounded read-only compatibility probe. The public
client accepts only an explicit IP-literal loopback socket with non-zero port,
positive connect/I/O deadlines no greater than 60 seconds and a caller-selected
response limit no greater than 4 KiB. It opens one connection, sends exactly
`ping\0`, reads through EOF under one absolute I/O deadline, parses a fixed JSON
DTO and accepts only `server == "VIIPER"` and `version == "0.7.0"`.

There is deliberately no default `3242` endpoint, hostname/DNS lookup,
discovery, environment override, arbitrary management request, resource CRUD or
device-stream constructor. All socket tests use a process-local
`TcpListener(127.0.0.1:0)` fixture.

## Fixed-revision finding and next decision

The v0.7.0 API documentation says a failed localhost auto-attach is logged but
does not affect the add response. The reviewed fixed-revision implementation can
instead return 409 after the device has already been created, without rollback.
This makes standalone add error handling ambiguous and means add cannot be used
as a read-only probe.

The affected mutating work stopped at that boundary. Before 003C2, an explicit
ADR amendment or follow-on decision must require evidence that local auto-attach
is disabled and define rollback/combined-error behavior. The safe design is one
owned operation spanning compatibility re-probe, bus creation, fixed Xbox 360
add, immediate stream handshake, initial neutral write and bounded cleanup; no
generic bus/device CRUD should escape that owner.

## Changed boundaries

- `adapters/viiper/src/client.rs`: loopback-only configuration, bounded TCP
  request/EOF response and strict compatible-ping DTO/error handling;
- `adapters/viiper/tests/probe_fixture.rs`: exact framing, configuration,
  identity/version, Problem, malformed/trailing/empty, N/N+1 size and no-EOF
  timeout evidence;
- VIIPER README, security/testing/third-party records and active plan: current
  claims, dependencies and the blocked mutating boundary.

The production dependency additions are already workspace-pinned `serde
1.0.229`, `serde_json 1.0.151` and `thiserror 2.0.20`, all `MIT OR
Apache-2.0`. They replace hand-written JSON/error parsing without importing an
upstream generated client; the byte/deadline bounds are applied before JSON
deserialization.

## Verification

- `cargo test -p capyio-viiper-adapter`: passed (15 tests).
- `cargo clippy -p capyio-viiper-adapter --all-targets -- -D warnings`:
  passed.
- `cargo xtask validate-docs`: passed (84 unique Requirement IDs).
- `cargo xtask validate-manifests`: passed (2 manifests).
- `cargo xtask ci`: passed, including workspace format/check/Clippy/tests,
  deterministic demo, repository validation, Adapter smoke and desktop
  typecheck/build.

## Explicit exclusions and residual risk

- no real VIIPER connection, start, download or executable import;
- no bus/device creation, stream handshake or USB/IP operation;
- no driver, certificate, Secure Boot, test-signing or system-device change;
- no Runtime/UI/Android integration and no interoperability claim;
- no commit, push, pull request or release operation.

The exact ping identity is self-reported by the peer. It is a compatibility
gate, not binary provenance, authentication or CapyIO authorization.

## Reviewed upstream evidence

- <https://github.com/Alia5/VIIPER/blob/v0.7.0/docs/api/overview.md>
- <https://github.com/Alia5/VIIPER/blob/v0.7.0/internal/server/api/handler/ping.go>
- <https://github.com/Alia5/VIIPER/blob/v0.7.0/internal/codegen/common/version.go>
- <https://github.com/Alia5/VIIPER/blob/v0.7.0/internal/server/api/handler/device.go>
