# CAPY-GAMEPAD-001B Report

Date: 2026-08-29

Status: bounded loopback transport and hardware-free validation complete

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-gamepad`

Branch: `codex/capyio-gamepad`

## Objective

Extend the transport-free CAPY-GAMEPAD-001A codec with one bounded, caller-
polled DSU UDP endpoint that can be exercised on loopback without claiming
phone, emulator or production-network interoperability.

## Boundary

- binds only IPv4 loopback (`127.0.0.1`);
- accepts port `0` for an OS-assigned test endpoint and never selects port
  `26760` implicitly;
- owns no thread, wall clock, Android sensor or Runtime Route lifecycle;
- preallocates one maximum-size UDP receive buffer at bind time;
- retains at most 16 subscribers in fixed storage;
- requires caller-supplied nondecreasing monotonic milliseconds;
- bounds subscriber TTL to 250ms through 30s and receive work to at most 64
  datagrams per poll;
- keeps only slot 0 available and emits neutral gamepad controls plus projected
  IMU motion;
- supports all-controller and slot-0 registrations. MAC-only registration does
  not match because this virtual projection deliberately advertises no stable
  hardware MAC.

The endpoint is a local, unauthenticated external-protocol projection. Loopback
restriction reduces exposure but does not create CapyIO pairing, authorization,
replay protection or production transport security.

## Implementation

- `DsuLoopbackConfig` validates fixed capacity, TTL and per-poll bounds.
- `DsuLoopbackServer::poll` handles version, port-info and pad-registration
  datagrams without blocking.
- malformed and oversized datagrams are counted and ignored.
- full registries reject only the new subscriber; existing subscriptions can
  renew.
- a client ID change on the same endpoint replaces the subscription and resets
  its per-client DSU packet number.
- expired subscriptions are removed before request processing or publishing.
- `publish_motion` sends one fixed 100-byte packet per active subscriber and
  reports would-block and send-error counters without failing unrelated peers.

## Files

- `adapters/dsu/src/transport.rs`
- `adapters/dsu/src/protocol.rs`
- `adapters/dsu/src/lib.rs`
- `adapters/dsu/tests/udp_loopback.rs`
- `adapters/dsu/README.md`
- `docs/CAPY_GAMEPAD_001B_REPORT.md`

No production dependency, root manifest, lockfile, platform driver, Android
permission or shared integration document is changed.

## Automated evidence

- loopback version and two-slot inventory responses;
- slot-0 subscription, renewal and expiry using logical time;
- deterministic IMU fixture delivery as sequential 100-byte DSU packets;
- malformed datagram isolation and MAC-only non-match;
- fixed subscriber-capacity rejection without evicting an existing client;
- same-endpoint client-ID replacement and packet-number reset;
- per-poll receive budget and monotonic-time regression behavior;
- targeted check/test/Clippy plus full `cargo xtask ci`.

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

The first full CI attempt hit an unrelated existing Windows ephemeral-port
flake in two `capyio-audio-share-adapter` tests: UDP selected a port that Windows
then rejected for the paired TCP bind with `WSAEACCES` (`10013`). An immediate
targeted rerun of all four Audio Share transport tests passed, and the complete
`cargo xtask ci` rerun passed. No Audio Share file was changed in this worktree.

## Remaining risks

- No Cemu/Dolphin process has discovered this endpoint.
- No live SensorServer/phone sample enters the endpoint.
- The correct physical mounting axis/sign mapping remains unmeasured.
- The caller must generate a suitable stable-per-run server ID and drive poll
  cadence, Route state and diagnostics.
- Loopback DSU is unauthenticated and must not be generalized to a LAN bind
  without a separate security and authorization design.
- Buttons, sticks, triggers, touch and reverse haptics remain later slices.

No commit or push was performed.
