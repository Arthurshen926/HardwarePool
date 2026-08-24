# CAPY-IMU-001B0 validation report

Date: 2026-08-24

Branch: `codex/capyio-sensorserver-contract`

Base: `CAPY-IMU-001A` commit `6393466770697341f96e5863cdaf5a384685e701`

## Result

The bounded SensorServer parser and IMU pairing contract is locally complete.
This is the first third-party protocol Adapter boundary, but it does not yet
open a WebSocket or claim live phone data.

## Implemented

- Resolved the normative conflict between the foundation PRD non-goals and the
  active Gate 5 Backlog through ADR 0021 and PRD v0.4-pre-alpha.
- Recorded upstream `UmerCodez/SensorServer` commit
  `5ae401780d99debcabb8dc259256c2652dada0a6`, GPL-3.0-only license and external
  service mode. No upstream source or binary is imported or distributed.
- Added a CapyIO-authored parser with a 4 KiB pre-decode bound, a closed JSON
  shape, exactly three finite axes, positive timestamps and Android accuracy
  values 0–3.
- Added deterministic one-use accelerometer/gyroscope pairing with configurable
  skew. Replaced unpaired readings, excessive skew, timestamp regression and
  sequence exhaustion are explicit.
- Extended IMU Profile v1 with optional component source timestamps. Existing
  fixtures omit the optional field and remain compatible.
- Added optional freshness-bounded magnetic-field inclusion and retained the
  least accurate required component rather than overstating accuracy.

## Validation

Passed locally:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                         # 88 tests passed
python scripts/validate_repository.py --self-test
python scripts/validate_repository.py          # 84 IDs + provenance
cargo xtask ci
pnpm --filter @capyio/desktop typecheck
pnpm --filter @capyio/desktop build             # 26 modules
```

The nine new SensorServer tests cover documented-shape parsing, input bounds,
either arrival order, one-use pairing, skew recovery, explicit replacement,
timestamp regression, optional magnetic data, component timestamp validation
and sequence exhaustion.

MSVC printed its normal desktop DLL import-library linker message during the
test build. Full Clippy with warnings denied passed; no source warning remains.

## Device and security boundary

- `adb devices -l` returned no device in this session.
- Reconnecting `100.66.157.119:42575` was actively refused, consistent with a
  disabled/restarted wireless-debug listener or changed port.
- No APK, permission, foreground service or phone setting changed.
- No network socket, WebSocket dependency, production authentication, driver,
  release, tag or package was added.
- Tailscale may protect overlay transit but is not CapyIO application identity,
  Route authorization or replay defense.

## Next slice

`CAPY-IMU-001B1` should add a reviewed Rust WebSocket client dependency, bounded
connection/read deadlines, an allow-listed `ws://host:port` local-lab endpoint,
mock WebSocket server tests and reconnect-to-new-epoch behavior. Physical phone
evidence remains `CAPY-IMU-001B2` after wireless ADB and SensorServer are online.
