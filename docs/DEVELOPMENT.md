# HardwarePool Development Guide

## 1. Bootstrap

```bash
git clone <repository-url>
cd hardwarepool
cargo xtask doctor
corepack enable
pnpm install
```

After the first successful online install, commit `Cargo.lock` and `pnpm-lock.yaml`.

## 2. Common commands

```bash
cargo xtask fmt
cargo xtask check
cargo xtask test
cargo xtask ci
cargo xtask demo
```

UI:

```bash
pnpm dev
pnpm typecheck
pnpm build
pnpm tauri dev
```

## 3. Branch and task flow

1. Create or select an issue with requirement IDs.
2. Add an execution plan for multi-step changes.
3. Create a focused feature branch.
4. Ask an Agent to implement one bounded task.
5. Require tests and command output.
6. Use another review pass or Agent for architecture/security review.
7. Run hardware tests when the change crosses a platform boundary.
8. Merge only after evidence is attached.

## 4. Code ownership boundaries

- `crates/hardwarepool-core`: deterministic domain logic only.
- `crates/hardwarepool-audio`: bounded frame/reorder/drift primitives; no sockets or platform APIs.
- `crates/hardwarepool-protocol`: generated protocol and conversion.
- `crates/hardwarepool-runtime`: orchestration and events; still OS-independent.
- `crates/hardwarepool-testkit`: fixtures, not production platform logic.
- `apps/gui`: shared UI and Tauri command boundary.
- `platform/*`: platform-specific user-mode adapters.
- `drivers/windows-audio`: Windows kernel project only.

## 5. Adding a capability Profile

1. Write requirements and a Profile document.
2. Define roles, permissions, projections, formats and errors.
3. Add typed Core model or opaque extension strategy.
4. Extend Protobuf append-only.
5. Add validation and compatibility tests.
6. Add platform Adapter interfaces.
7. Add UI representation only after semantics are stable.

## 6. Adding a dependency

A task report must state:

- package and version range;
- why it is needed;
- alternatives considered;
- license;
- maintenance/security status;
- whether it enters real-time, network, privileged or kernel paths.

## 7. Agent review checklist

Ask the review Agent to inspect:

- requirement compliance;
- dependency direction;
- illegal state transitions;
- unbounded queues and allocations;
- protocol compatibility;
- permission and privacy changes;
- unsafe/high-risk system commands;
- missing tests or false test claims.

## 8. Platform work

Platform work should begin with a small Audio Lab or Driver Spike. Do not combine initial platform access, networking, UI and production security in one task. Preserve logs and measurements in `test-results/` outside Git unless sanitized summaries are committed.
