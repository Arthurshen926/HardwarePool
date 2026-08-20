# Repository Map

## Executable entry points

- `apps/hardwarepool-node` — deterministic headless CLI demo and protocol round-trip.
- `apps/gui` — Vue control surface; browser mode uses a deterministic mock API.
- `apps/gui/src-tauri` — Tauri host that drives the same Rust demo Runtime.
- `xtask` — repository doctor, formatting, checks, tests and demo commands.

## Shared Rust crates

- `hardwarepool-core` — identifiers, capability/Profile model, audio negotiation values, authorization leases and independent binding state machines.
- `hardwarepool-audio` — bounded audio-frame validation, reordering/loss accounting and clock-drift estimation.
- `hardwarepool-protocol` — Protobuf v1 generation, binary Envelope codec and explicit Core conversion.
- `hardwarepool-runtime` — peer/session registry, commands, events and UI snapshots.
- `hardwarepool-testkit` — deterministic Windows/Android fixtures used by CLI, tests and UI.

## Contracts and future implementations

- `protocol/proto/hardwarepool/v1` — authoritative control schema.
- `drivers/windows-audio` — future Windows kernel projection and IPC contract.
- `platform/android` — future Android permissions, service and audio Adapter.
- `platform/windows` — future Windows user-mode Broker.
- `platform/linux`, `platform/macos` — future platform adapters.

## Normative documents

- Product scope: `docs/PRODUCT_REQUIREMENTS.md`
- Architecture: `docs/ARCHITECTURE.md`
- Audio semantics: `docs/AUDIO_PROFILE.md`
- Protocol rules: `docs/PROTOCOL.md`
- Security: `docs/SECURITY_MODEL.md`
- Testing: `docs/TESTING.md`
- Agent rules: `AGENTS.md` plus the nearest directory-specific file

## Agent execution aids

- `docs/BACKLOG.md` — ordered, acceptance-driven task queue.
- `docs/plans/TEMPLATE.md` — versioned execution-plan template.
- `scripts/new_agent_task.py` — creates an active plan without overwriting existing work.
- `.vscode/tasks.json` — safe, non-privileged bootstrap commands.
