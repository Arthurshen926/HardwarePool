# xtask Agent Rules

- Commands are cross-platform and safe by default.
- `doctor` is read-only and never installs or reconfigures tools.
- Never add driver deployment, `bcdedit`, Verifier, signing, device reset or credential operations to an automatically run command.
- Keep CI/local commands aligned; report skipped optional checks explicitly.
