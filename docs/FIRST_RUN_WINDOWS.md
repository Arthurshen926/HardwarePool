# First Run on Windows

The first section validates CapyIO without drivers, APKs or physical hardware.
Optional controlled-lab sections then explain how to use already provisioned
real capabilities without silently installing or changing the host.

## Prerequisites

Install the ordinary development tools listed under `required-now` in
`docs/TOOLCHAIN.md`. Android/WDK/media tools are needed only for the associated
controlled-lab work.

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

The browser UI is simulated: its Nodes, metrics and Route state do not represent
physical devices.

## Desktop Tauri shell

```powershell
corepack pnpm tauri dev
```

The Tauri backend uses the same DTO contract and deterministic Rust testkit. On
an explicitly provisioned lab host it also discovers real IMU, Speaker and
microphone Quick Actions. Missing host configuration is reported as a blocked
action rather than triggering an install.

## Use an already provisioned Android microphone

The complete operator flow, expected Windows endpoint labels, privacy behavior
and troubleshooting checks are documented in
[`MICROPHONE_SHARING_WINDOWS_ANDROID.md`](MICROPHONE_SHARING_WINDOWS_ANDROID.md).
The short path is:

1. validate the trusted MicYou host configuration;
2. start the Tauri desktop and press **启动** on **将手机麦克风用作电脑麦克风**;
3. connect Android MicYou to the address shown on that card;
4. wait for `Active`, then select the CapyIO microphone in the recording or
   meeting application;
5. press **停止** when finished.

This path uses the already installed lab driver/APK. It does not make the
workspace a production installer or support unattended distribution.

## Stop point

Do not install drivers, enable test signing/Verifier, change boot security,
generate/install an APK, connect a personal phone, or add Android permissions
without the task and approvals required by root `AGENTS.md`.
