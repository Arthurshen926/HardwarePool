# First Run on Windows

This sequence validates the CapyIO foundation without drivers, APKs or physical
hardware.

## Prerequisites

Install only ordinary development tools listed under `required-now` in
`docs/TOOLCHAIN.md`. Android/WDK/media tools are optional for later Gates.

## Validate

```powershell
$env:Path = 'C:\Users\arthu\.cargo\bin;C:\Program Files\nodejs;' + $env:Path
cargo xtask doctor
python .\scripts\validate_repository.py
corepack pnpm install --frozen-lockfile
cargo xtask ci
```

## Run deterministic demos

```powershell
cargo xtask demo
cargo xtask adapter-smoke
cargo run -p capyio-node -- protocol-roundtrip
corepack pnpm dev
```

Expected UI behavior is simulated Quick Actions/Workspace with two symmetric
Nodes and four independently controlled Routes. No real device, network or
system Projection is used.

## Desktop Tauri shell

```powershell
corepack pnpm tauri dev
```

The Tauri backend uses the same DTO contract and deterministic Rust testkit.

## Stop point

Do not install drivers, enable test signing/Verifier, change boot security,
generate/install an APK, connect a personal phone, or add Android permissions
without the task and approvals required by root `AGENTS.md`.
