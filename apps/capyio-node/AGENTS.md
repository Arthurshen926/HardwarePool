# Headless node Agent Rules

- This binary is a thin composition layer; domain behavior belongs in Core/Runtime.
- Do not add platform SDK calls here.
- Demo mode must remain deterministic and clearly separate from production authentication.
- CLI output used by tests should prefer structured JSON over human-only parsing.
