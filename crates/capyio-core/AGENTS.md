# Core-specific Agent Rules

- This crate is deterministic domain logic only.
- Do not add async runtimes, sockets, filesystem access, environment reads, platform APIs, Tauri, codecs or generated Protobuf types.
- All public lifecycle methods require success and invalid-transition tests.
- Prefer typed identifiers and enums over strings.
- A new Profile-specific type requires a matching Profile document and validation.
- Route state is independent; one Route transition must not mutate another.
- Direction belongs to Port and there is no NodeRole, LocalRole or StreamRole.
- Serialization derives are for diagnostics/persistence experiments, not an automatic wire-compatibility promise.
