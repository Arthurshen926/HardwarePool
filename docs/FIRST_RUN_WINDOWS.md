# First Run on the Windows Development Machine

This sequence validates the bootstrap repository without installing a driver or accessing the phone microphone.

## 1. Extract and initialize locally

```powershell
Expand-Archive .\hardwarepool-bootstrap.zip -DestinationPath .
Set-Location .\hardwarepool-bootstrap

git init
git add -- .
git status
```

Review the staged files before the first commit. Repository publication, commit, push and remote creation remain separate human decisions.

## 2. Install ordinary development prerequisites

Use the pinned baselines in `docs/TOOLCHAIN.md`:

- Git;
- Rust via rustup;
- Node.js and Corepack;
- Python 3;
- Microsoft C++ Build Tools/WebView2 prerequisites for Tauri desktop work.

Android SDK/NDK and WDK can wait until their corresponding gate.

## 3. Read-only environment checks

```powershell
PowerShell -ExecutionPolicy Bypass -File .\scripts\bootstrap-windows.ps1
cargo xtask doctor
python .\scripts\validate_repository.py
```

These commands do not install drivers or change boot/security settings.

## 4. First online dependency resolution

```powershell
corepack enable
pnpm install
cargo metadata --format-version 1 | Out-Null
```

This should create `pnpm-lock.yaml` and `Cargo.lock`. Inspect dependency changes, then retain both lockfiles.

## 5. Compile the OS-independent bootstrap

```powershell
cargo xtask fmt
cargo xtask check
cargo xtask test
cargo run -p hardwarepool-node -- demo
cargo run -p hardwarepool-node -- protocol-roundtrip
cargo run -p hardwarepool-node -- audio-frame-demo
```

Do not start new feature work until failures are captured in a focused HP-BOOT issue and repaired.

## 6. Build the shared browser UI

```powershell
pnpm typecheck
pnpm build
pnpm dev
```

Expected behavior:

- one Android peer is shown;
- microphone and speaker are separate cards;
- either card can be toggled independently;
- metrics and warnings clearly say that data is simulated.

## 7. Build the Tauri demo

```powershell
pnpm tauri dev
```

Expected behavior is the same, except `backendMode` is `tauri_demo` and commands operate on the Rust `DemoLab`.

## 8. Stop point

At this point the repository is validated as a shared Core/Protocol/UI bootstrap. Do not install test drivers, change Android permissions, run `bcdedit`, enable Driver Verifier or alter Secure Boot/BitLocker. Continue with the earliest open task in `docs/BACKLOG.md`.
