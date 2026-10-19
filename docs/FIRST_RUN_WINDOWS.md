# First Run on Windows

## Local Alpha user path

The current installable candidate, exact hashes and validation limits are in
[the local Alpha delivery](LOCAL_ALPHA_CURRENT.md). Use that record
when choosing a package; older lab reports are historical evidence.

1. Use the Windows account that owns the existing pairing. Before switching
   versions, choose **节点 → 退出节点** in the old application. Closing its window
   leaves the Host running; reopening the UI attaches to that Host.
2. Install the Core application into a new directory, or unpack the complete Core
   archive. Keep its helpers alongside `capyio-desktop.exe`.
3. In the installed directory, run the read-only checks:

   ```powershell
   .\verify_local_debug_bundle.ps1 -PackageRoot .
   .\check_local_prerequisites.ps1
   ```

4. Open `capyio-desktop.exe` and the unified CapyIO Android app. For new pairing,
   use **设置** and the PIN pairing panel. Keep both devices on trusted LAN/Tailscale.
5. Select the needed resource in **远程资源**, open its use panel and start it.
   For microphone/camera projection, select the CapyIO endpoint in the receiving
   recording or meeting application. Stop the resource in CapyIO when finished.

The application installer copies files only. Broker, audio/VHF drivers and camera
registration must already be provisioned; a successful installation does not
establish their readiness. The updated workspace script checks speaker and microphone presence separately;
the existing candidate still contains the earlier inventory script. The states below
apply to the updated script and will enter the next package.
`Device error` reports a Windows device error code; `Health unknown` means the
inventory did not provide one. `Detected` is presence evidence, not media acceptance.
Pairing is not inspected by this script. Missing components require the separate,
target-specific deployment process; do not change boot security to use this pack.

Camera currently requires an elevated Host. If a normal Host is already running,
exit that node before launching the application with administrator privileges.
Gamepad, display, haptics and IMU remain experimental; Core does not include all
optional backends. Android APK installation remains a separately authorized step.

## Developer prerequisites and validation

Install development tools listed under `required-now` in
[TOOLCHAIN.md](TOOLCHAIN.md). Android/WDK/media tools are needed only for their
associated work; they are not prerequisites for using an installed application.

```powershell
$env:Path = (Join-Path $env:USERPROFILE '.cargo/bin') + ';' + (Join-Path $env:ProgramFiles 'nodejs') + ';' + $env:Path
cargo xtask doctor
python .\scripts\validate_repository.py
corepack pnpm install --frozen-lockfile
cargo xtask ci
```

```powershell
cargo xtask demo
cargo xtask adapter-smoke
cargo run -p capyio-node -- protocol-roundtrip
corepack pnpm dev
```

The browser UI and deterministic demos are simulated; their Nodes, metrics and
Route states do not represent physical devices. Run the native desktop with
`corepack pnpm tauri dev` for the platform backend.

## Compatibility and physical evidence

MicYou/Audio Share are retained compatibility diagnostics, not the default native
microphone/speaker flow. Their historical instructions are in
[MICROPHONE_SHARING_WINDOWS_ANDROID.md](MICROPHONE_SHARING_WINDOWS_ANDROID.md).
The [physical acceptance guide](PHYSICAL_ACCEPTANCE_GUIDE.md) retains older test
procedures; current evidence and unverified items are linked from
[CURRENT_STATUS.md](CURRENT_STATUS.md).

Driver deployment, personal-device APK installation, permission/foreground-service
changes and production signing remain subject to root `AGENTS.md`. Building an
uninstalled debug APK is a software validation step, not a device installation.
