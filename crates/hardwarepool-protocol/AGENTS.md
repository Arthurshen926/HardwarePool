# Protocol-specific Agent Rules

- `.proto` files are canonical; generated Rust is never committed or hand-edited.
- Never reuse or reinterpret a field number.
- Core types and wire types remain separate; add explicit conversion and validation.
- Unknown/zero enum values must not silently become a valid domain value.
- Control messages must not contain raw high-frequency audio payloads.
- Protocol changes require tests and an update to `docs/PROTOCOL.md`; breaking semantic changes require an ADR and major-version decision.
