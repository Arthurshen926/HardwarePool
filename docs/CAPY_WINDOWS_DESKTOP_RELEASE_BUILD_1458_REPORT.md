# CapyIO Windows Desktop release-build report (1458)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`
- Build command: `pnpm --filter @capyio/desktop tauri build --no-bundle`
- Result: PASS

## Artifact

- Path: `target/release/capyio-desktop.exe`
- Size: 7,262,208 bytes
- SHA-256: `9B3985C0D487BA86379EBB706F017CDA464962E181EC7E44893738A6E484425A`

The executable was launched hidden on the build host, remained alive for five
seconds and was then stopped by its exact process ID. This is a startup smoke,
not an installer, signature, upgrade, firewall, long-duration or physical
Android interoperability result. The release build emitted only existing
dead-code warnings for debug physical-gamepad gate helpers plus the MSVC import-
library linker notice.

Tauri bundling remains disabled, so there is intentionally no MSI/NSIS package.
The executable is an uncommitted build artifact under `target/` and can be
reproduced from the recorded source commit and command.
