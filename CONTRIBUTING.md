# Contributing to CapyIO

CapyIO is currently in architecture-bootstrap stage. Contributions should preserve the separation between shared Core semantics, versioned Profiles, transport bindings, platform adapters, and system-device projections.

## Before coding

1. Read `docs/PRODUCT_REQUIREMENTS.md`, `docs/ARCHITECTURE.md`, and `AGENTS.md`.
2. Search for an existing issue or plan.
3. For an architectural or protocol change, add or update an ADR first.
4. Write acceptance criteria before implementation.

## Local workflow

```bash
cargo xtask doctor
cargo xtask fmt
cargo xtask check
cargo xtask test
```

For the UI:

```bash
corepack enable
pnpm install
pnpm typecheck
pnpm build
```

Do not install test drivers on a daily-development machine. Driver changes require the workflow in `drivers/windows-audio/README.md`.

## Pull requests

- Keep changes focused and reviewable.
- Link requirement IDs and issues.
- Include tests and documentation updates.
- Report every command executed and its result.
- Note hardware-dependent tests that were not run.
- Never commit keys, certificates, recordings containing private speech, crash dumps containing secrets, or generated build directories.

## Commit convention

Use conventional prefixes where practical:

```text
feat: add capability negotiation state
fix: reject unsupported audio format
refactor: isolate protocol conversion
 test: cover independent microphone shutdown
docs: record driver-broker boundary
chore: update CI toolchain
```
