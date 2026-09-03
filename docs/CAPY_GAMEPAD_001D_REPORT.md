# CAPY-GAMEPAD-001D report

Date: 2026-08-29

Branch: `codex/capyio-gamepad`

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

## Result

`capyio-windows-input` now owns one typed StandardPort IMU-to-DSU
`ExternalProtocol` Route and one bounded loopback Worker. Runtime `Starting`
selects the fixed stream epoch; exact anchor validation and UDP bind precede
`Active`. Upstream disconnect and explicit stop join the Worker and release its
port before Offline/Stopped, while retry uses a strictly newer epoch.

Queue pressure remains a non-blocking observable outcome. A stopped or failed
Worker becomes a typed projection Problem. Bind failure belongs to the DSU
Adapter, while source epoch/disconnect failures belong to the source Adapter.
Every injected failure proves that a simultaneously Active unrelated IMU Route
and its diagnostics remain unchanged.

## Dependency note

The Windows composition crate adds only internal Apache-2.0 workspace
dependencies on `capyio-data-plane` and `capyio-dsu-adapter`. They share the
repository MSRV, maintenance policy and CI. No third-party production package
was added. Reimplementing envelopes or the DSU Worker in the platform crate
would split the existing contract/lifecycle authority and was rejected.

## Automated evidence

- three DSU Runtime fixture tests cover retry/port release, occupied-port
  rollback, epoch mismatch, typed ownership and unrelated-Route isolation;
- the existing four VIIPER Runtime fixtures continue to pass;
- focused format, check, test and Clippy-with-warnings-denied pass;
- `cargo xtask ci` passes the full workspace format, check, Clippy, tests,
  deterministic IMU demo, documentation/manifests, Adapter smoke, structural
  validation and desktop typecheck/build gates.

No phone, emulator, external process, USB/IP or driver was used.
