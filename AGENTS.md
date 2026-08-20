# HardwarePool Agent Instructions

## Mission

HardwarePool is a cross-platform distributed hardware capability platform. The current MVP shares an Android phone's speaker and microphone with Windows as independent, system-level audio endpoints.

## Read before changing code

1. `docs/PRODUCT_REQUIREMENTS.md`
2. `docs/ARCHITECTURE.md`
3. `docs/AUDIO_PROFILE.md`
4. `docs/PROTOCOL.md`
5. `docs/SECURITY_MODEL.md`
6. `docs/TESTING.md`
7. The nearest directory-specific `AGENTS.md`

The documents above are normative. When code and documentation disagree, do not silently choose one: stop the change, document the mismatch, and propose an ADR.

## Architecture invariants

- `hardwarepool-core` must not depend on Windows SDK, WDK, Android SDK, Tauri, UI frameworks, codecs, or a concrete network transport.
- Microphone and speaker are separate capability instances. A duplex bundle references them; it does not replace them.
- Core lifecycle semantics are shared. Hardware-specific semantics live in versioned Profiles.
- Platform implementations are behind explicit traits or process boundaries.
- Network, pairing, codecs, Protobuf/JSON parsing, and reconnect logic must never execute in the Windows kernel driver.
- A Windows driver must expose only the minimum PCM/endpoint/IPC surface required by the Broker.
- Real-time audio callbacks must not block, allocate, acquire contended locks, perform file I/O, or emit normal logs.
- Protocol fields are append-only inside a major version. Never reuse a removed field number.
- Unknown enum values and newer Profile versions must fail explicitly or be preserved as opaque data; never reinterpret them silently.
- Remote disconnect must not crash or hang an operating-system audio service.
- No platform may start microphone capture without the platform-required visible permission and lifecycle state.

## Scope rules

- Complete one small issue at a time.
- Do not add cameras, sensors, WAN relay, mesh routing, ASIO, spatial audio, or multi-phone mixing unless the active plan explicitly includes them.
- Do not add a production dependency without recording its purpose, license, maintenance status, and alternatives in an ADR or dependency note.
- Do not change public protocol or Core types only to simplify one UI screen or platform workaround.

## Standard commands

```text
cargo xtask doctor       # inspect local prerequisites
cargo xtask fmt          # format Rust and frontend sources
cargo xtask check        # static checks without driver deployment
cargo xtask test         # unit and integration tests
cargo xtask ci           # local approximation of required CI
cargo xtask demo         # run the deterministic mock node flow
```

When a command cannot run because the environment lacks a dependency, report the exact missing dependency and preserve the command output in the task summary.

## Definition of done

A change is complete only when:

- Its acceptance criteria are satisfied.
- Changed behavior has tests or a documented reason why automated testing is impossible.
- Required checks pass in the environments that are available.
- Protocol/architecture/security documentation is updated when relevant.
- The final task report lists changed files, commands executed, evidence, and unresolved risks.

## High-risk operations

Never perform these without explicit human approval and a verified target:

- installing or removing a Windows driver;
- running `bcdedit`, `verifier`, `pnputil`, `devcon`, `signtool`, or changing Secure Boot/BitLocker settings;
- installing an APK on a personal device;
- changing Android permissions or foreground-service declarations;
- using production signing keys or certificates;
- publishing packages, releases, Git tags, commits, pushes, or pull requests;
- deleting user data or resetting a physical device.

Driver tests must target an isolated VM or dedicated test Windows installation. The daily-development Windows host is not a driver test target.

## Task request template

Every non-trivial task should identify:

- Task ID
- Goal
- Source documents / requirement IDs
- In scope
- Out of scope
- Acceptance criteria
- Required tests
- Allowed dependency changes
- Safety constraints
- Deliverables

A reusable template is in `docs/AGENT_WORKFLOW.md`.
