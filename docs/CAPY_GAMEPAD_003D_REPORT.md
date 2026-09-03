# CAPY-GAMEPAD-003D report

Date: 2026-08-29

Branch: `codex/capyio-gamepad`

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

## Result

The Windows input composition boundary now installs one typed
`capyio.input.gamepad-state/1` Source-to-VIIPER Sink Route and owns its VIIPER
Xbox 360 Worker. The Route moves through Draft, Prepared and Starting, and
becomes Active only after exact compatibility probing, bus/device provisioning,
stream handshake and initial neutral all succeed.

The Runtime Route epoch becomes the fixed gamepad stream epoch. Upstream
disconnect cleans the Worker before reporting Offline; retry creates a fresh
bus/stream under a strictly newer epoch. Explicit stop likewise completes
neutral, socket shutdown and owned-bus removal before Stopped. Open failure,
terminal stream failure and sequence exhaustion report typed, retryable
gamepad Problems. Unsupported controls remain a non-terminal frame rejection
and do not consume sequence.

Bounded raw two-byte rumble polling is exposed only so a closed device stream
can drive lifecycle failure. No duration is invented and no reverse haptics
Route is constructed; that remains `CAPY-GAMEPAD-004` scope.

## Architecture decision

Composition lives in `capyio-windows-input`, not `capyio-input`,
`capyio-runtime` or `capyio-viiper-adapter`. This preserves the portable input
contract, deterministic Runtime and protocol-only Adapter dependency directions
from ADR 0040 and ADR 0042. The controller registers a StandardPort Sink and
uses `ExternalProtocol`; it does not claim an arbitrary `AdapterManaged`
interop contract.

## Dependency note

The Windows input crate adds only internal Apache-2.0 workspace dependencies:
`capyio-core` for typed catalog/Route identities, `capyio-runtime` for the
authoritative lifecycle and `capyio-viiper-adapter` for the bounded owned
session. They share this repository's MSRV, maintenance policy and CI. The
existing `capyio-input` dependency remains the semantic contract, and
`capyio-testkit` is development-only. No third-party production package was
added. Duplicating Runtime or VIIPER logic would split lifecycle authority;
placing the composition in the desktop app would couple a Windows Projection
to Tauri, so both alternatives were rejected.

## Automated evidence

- `cargo test -p capyio-windows-input`: passed (4 fixture tests).
- `cargo test -p capyio-viiper-adapter`: passed (22 tests).
- `cargo clippy -p capyio-windows-input --all-targets -- -D warnings`: passed.
- `cargo xtask validate-docs`: passed (84 unique Requirement IDs).
- `cargo xtask validate-manifests`: passed (2 manifests).
- `cargo xtask ci`: passed, including workspace format/check/Clippy/tests,
  deterministic demo, repository validation, Adapter smoke and desktop
  typecheck/build.

One intermediate full-CI rerun saw Windows `WSAEACCES (10013)` while two
unrelated Audio Share tests bound a local TCP socket. Their focused rerun
passed immediately, and the subsequent complete CI run passed. No audio code
was changed in this slice; the transient host bind result remains test-environment
evidence rather than a gamepad failure.

Fixture coverage includes two-session Route retry with strictly increasing
epoch, exact neutral/button/cleanup order, non-terminal unsupported controls,
add rollback, sequence exhaustion, peer stream close and gamepad-only Problems.
Every injected failure runs while an existing IMU Route is Active and proves
that its state and diagnostics remain unchanged.

## Explicit exclusions

- no real VIIPER process connection, start, configuration or binary import;
- no USB/IP attach, driver/certificate/security-policy or system-device change;
- no UI/Android integration, controller enumeration or real game claim;
- no reverse haptics Route or duration/lifecycle mapping;
- no commit, push, pull request or release operation.
