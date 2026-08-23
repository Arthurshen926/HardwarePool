# CapyIO Development

## Safe local loop

```text
cargo xtask doctor
cargo xtask fmt
cargo xtask check
cargo xtask test
cargo xtask demo
```

Before a local foundation commit:

```text
cargo xtask validate-docs
cargo xtask validate-manifests
cargo xtask adapter-smoke
cargo xtask ci
```

Frontend:

```text
corepack pnpm install --frozen-lockfile
corepack pnpm typecheck
corepack pnpm build
corepack pnpm dev
```

The demo and browser UI are deterministic and simulated.

## Change sequence

1. Read the active plan and nearest `AGENTS.md`.
2. Identify Requirement IDs and architecture/ADR constraints.
3. Record baseline behavior when the change is non-trivial.
4. Implement one bounded slice with success and rejection tests.
5. Run the smallest relevant checks immediately.
6. Run the full Gate checks before a commit.
7. Update evidence and risks; do not claim unrun hardware tests.

## Dependencies

Do not add a production dependency without recording purpose, license,
maintenance status and considered alternatives. Third-party vertical code also
requires a `third_party/THIRD_PARTY.yml` entry with a pinned revision before
import.

## Protocol and manifests

- edit `.proto` and JSON Schema sources, never generated Rust/Tauri schema files;
- never reuse a removed Protobuf field number;
- add conversion, unknown-version and malformed-input tests;
- keep high-rate data out of control messages;
- keep Adapter stdout machine-only and send logs to stderr.

## Platform and hardware work

Pure foundation commands do not install system components. APK installation,
Android permission/service declarations, physical devices, Windows drivers,
boot/security configuration, signing, releases and remote publication require
the approvals documented in root `AGENTS.md`.

## Task evidence

Each non-trivial task records implemented files, exact commands/results,
unvalidated platform behavior and the next risk. Hardware evidence belongs under
`test-results/<run-id>/` and records commit, OS/device versions and configuration.
