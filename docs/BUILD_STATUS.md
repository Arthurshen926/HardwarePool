# Bootstrap Build Status

Created: 2026-08-20
Last local validation: 2026-08-20 on Windows build 26200, x86_64
Last hosted validation: 2026-08-21 on GitHub Actions

## Creation environment

- Linux container, x86_64;
- Node.js 22.16.0 and global TypeScript 5.8.3 available;
- Rust/Cargo not installed;
- no network access from the container, so Rust/npm dependencies could not be downloaded;
- no Windows SDK/WDK, Android SDK/NDK, physical device or Windows test VM.

## Validation completed during archive creation

- `python3 scripts/validate_repository.py`: **PASS**;
- required-file and Cargo workspace-member inventory;
- UTF-8 and LF line-ending validation;
- no symbolic links, generated caches or obvious committed secrets;
- JSON, TOML and GitHub Actions/issue-form YAML parsing;
- Python source syntax validation;
- Protobuf package, brace and field-number structural validation;
- requirement-ID uniqueness validation: 65 normative requirements;
- Core/platform/driver dependency-boundary checks;
- local Markdown-link validation;
- standalone TypeScript compilation of the UI DTO and browser Mock Backend: **PASS**;
- `scripts/new_agent_task.py` smoke test: **PASS**;
- SHA-256 repository manifest generation and verification: **PASS**;
- final ZIP central-directory and payload integrity check: **PASS**.

## First Windows validation completed

Toolchain established:

- Rust/Cargo 1.97.1 with rustfmt and Clippy;
- Node.js 24.19.0, Corepack 0.35.0 and pnpm 11.5.3;
- Python 3.12.10 with PyYAML 6.0.3;
- Visual Studio 2022 Build Tools 17.14.37614 with VC Tools and Windows SDK;
- generated `Cargo.lock` and `pnpm-lock.yaml`.

Validated behavior:

- `cargo xtask doctor`: **PASS** for all required bootstrap tools;
- `cargo xtask ci`: **PASS**;
- Rust formatting, workspace check and Clippy with warnings denied: **PASS**;
- 28 Rust unit/integration tests: **PASS**; all doc tests: **PASS**;
- vendored Protobuf generation and Rust protocol compilation: **PASS**;
- deterministic CLI `demo`, `snapshot`, `protocol-roundtrip` and
  `audio-frame-demo`: **PASS**;
- repository structural validation including YAML parsing: **PASS**;
- pnpm install, Vue typecheck and Vite 8/Oxc production build: **PASS**;
- Browser Mock runtime: **PASS**; starting microphone produced `1 / 2` while
  speaker remained `not_mapped`;
- Tauri Rust target check and `tauri build --debug --no-bundle`: **PASS**;
- `pnpm tauri dev --no-watch`: **PASS** in the desktop-user context;
- Tauri Demo UI: **PASS**; both capabilities reached `2 / 2`, then stopping
  microphone left speaker active and produced separate Runtime events.

Bootstrap repairs made from first-build evidence:

- changed unavailable `@tauri-apps/api` 2.11.2 to published 2.11.1 (ADR 0007);
- migrated the Vite 8 build from deprecated esbuild minification to Oxc
  (ADR 0008);
- excluded downloaded/build directories from structural source validation;
- taught xtask to execute Corepack/pnpm `.cmd` shims on Windows;
- added generated Tauri icon resources required by Windows builds;
- removed the Tauri bundle-identifier and Windows PDB-name warnings.

One informational Rust warning remains during Tauri linking: MSVC reports the
creation of the DLL import library through `linker_messages`. It is not a
source/Clippy warning and the executable links successfully.

## First hosted validation completed

The repository baseline and CI repair were committed and pushed to the private
GitHub repository. Commit `4a658d5` completed all required hosted workflows:

- Rust core matrix on Windows, Linux and macOS: **PASS**;
- Shared UI typecheck and production build on Linux: **PASS**;
- static repository validation on Linux: **PASS**.

The first hosted run exposed and verified repairs for PowerShell line-ending
normalization and pnpm-before-cache setup ordering. Gate 0 is complete.

## Validation still pending

- release-mode Tauri bundles, installers, signing and reproducible-build checks;
- Android generation/build or physical-device tests;
- Windows Broker/driver build, deployment or Driver Verifier;
- real network, PCM, latency, clock-drift and acoustic tests.

The Android, network, Broker and driver items remain outside Gate 0. No driver,
APK, microphone permission, device deployment or security-setting operation was
performed during this validation.

## First local validation

Follow `docs/FIRST_RUN_WINDOWS.md`. The merge-gate approximation is:

```bash
cargo xtask doctor
python scripts/validate_repository.py
cargo xtask fmt
cargo xtask check
cargo xtask test
cargo xtask demo
corepack enable
pnpm install
pnpm typecheck
pnpm build
pnpm tauri dev
```

The commands above pass locally, and the corresponding Core, UI and static
checks pass in hosted CI. Gate 0 is closed.
