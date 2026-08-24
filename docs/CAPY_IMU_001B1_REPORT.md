# CAPY-IMU-001B1 Completion Report

Date: 2026-08-24

Status: complete locally

## Outcome

The SensorServer Adapter now owns a bounded synchronous WebSocket client for
the trusted local-lab path. It accepts only a typed IP address and non-zero
port, constructs fixed Android sensor paths, applies connect/read/write
deadlines and limits WebSocket frames and messages to 4 KiB. Only text frames
enter the strict parser from `CAPY-IMU-001B0`.

Connection lifecycle is explicit. Read timeout leaves the client open for a
caller-controlled retry. Close marks it closed; capacity and protocol failures
mark it failed. A closed or failed client cannot be read again, so later Runtime
orchestration must create a fresh client and advance the stream epoch.

## Dependency decision

The user approved `tungstenite` 0.30.0. Default features are disabled and only
`handshake` is enabled. No Tokio, native TLS or Rustls stack was added. Purpose,
license (`MIT OR Apache-2.0`), alternatives and boundary consequences are
recorded in ADR 0022 and `third_party/THIRD_PARTY.yml`.

## Evidence

The seven loopback WebSocket tests cover:

- fixed IPv4/IPv6 endpoint construction and invalid configuration;
- exact valid text-frame mapping;
- ping/pong and close reply handling;
- read timeout distinct from malformed JSON;
- binary and oversized data rejection;
- oversized handshake rejection;
- terminal closed/failed client non-reuse.

The SensorServer Adapter now has 16 integration tests: nine parser/pairing and
seven WebSocket tests. The workspace contains 95 passing Rust tests.

Commands passed:

```text
python scripts/validate_repository.py --self-test
python scripts/validate_repository.py
cargo fmt --all -- --check
cargo check --workspace --exclude capyio-desktop --all-targets
cargo clippy --workspace --exclude capyio-desktop --all-targets -- -D warnings
cargo test --workspace --exclude capyio-desktop
cargo xtask ci
```

The final `cargo xtask ci` also passed requirement/document validation,
manifest validation, Adapter Smoke, the deterministic IMU fixture demo, and
desktop frontend typecheck/build. The first CI attempt reached all Rust gates
but could not find `node`; rerunning with the installed Node.js directory on
`PATH` passed the full command.

## Physical-device status and remaining risk

Wireless ADB reconnected to the previously trusted Android device without a
new pairing operation. No pairing code was used in this slice. This does not
prove a live SensorServer connection: no APK was installed or changed, no
Android permission/service was changed, and no physical sensor payload was
captured.

The lab client uses plaintext `ws://`. Tailscale may protect the network path,
but it is not CapyIO application authentication or authorization. This path is
therefore not a production transport claim.

## Next slice

`CAPY-IMU-001B2` should verify/install the external SensorServer app under the
existing device authorization, start its service, connect accelerometer and
gyroscope streams through this client, retain bounded live Panel/Recorder
evidence, and record disconnect/reconnect behavior. Runtime/Tauri integration
remains `CAPY-IMU-001B3`; audio remains later work after the IMU Gate 5 path is
closed.
