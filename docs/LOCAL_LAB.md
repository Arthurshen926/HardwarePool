# Initial Local Lab

## 1. Available devices

- HP OmniBook Ultra Flip 14: development host and later Windows consumer.
- vivo X200 Pro mini: Android provider test device.

Product names are not sufficient for build decisions. Record exact versions before platform work.

## 2. Windows inventory

Run in PowerShell and store sanitized output outside Git or under `test-results/`:

```powershell
winver
Get-ComputerInfo | Out-File windows-computer-info.txt
systeminfo | Out-File windows-systeminfo.txt
Get-CimInstance Win32_ComputerSystem | Format-List *
Get-BitLockerVolume
Confirm-SecureBootUEFI
```

Record:

- Windows edition/build;
- x64 or ARM64 system type;
- CPU/RAM;
- Wi-Fi adapter and driver;
- Hyper-V availability;
- BitLocker and Secure Boot state.

Do not change BitLocker/Secure Boot for Bootstrap Gates.

## 3. Android inventory

```bash
adb devices
adb shell getprop ro.product.model
adb shell getprop ro.build.version.release
adb shell getprop ro.build.version.sdk
adb shell getprop ro.build.fingerprint
adb shell dumpsys audio > android-audio.txt
adb shell dumpsys media.audio_flinger > android-audioflinger.txt
```

Sanitize device identifiers before sharing logs publicly.

## 4. Windows driver target

Preferred:

- Hyper-V Generation 2 VM when Windows edition supports it;
- external virtual switch for phone connectivity;
- checkpoints for clean OS, toolchain and working sample baseline;
- WinDbg connection from host;
- test-signing and Driver Verifier only inside target.

Before any boot-policy change, save the recovery information and obtain explicit approval.

## 5. Network setup

Initial application-level audio uses a trusted local network and manual IP. Record:

- access-point model/band;
- 2.4/5/6 GHz;
- phone and PC IP;
- whether client isolation is enabled;
- baseline ping/jitter;
- firewall rules added for test executable.

## 6. First lab sequence

1. Validate Core/UI builds without devices.
2. Generate Tauri Android project.
3. Build Android Audio Lab with local capture/render only.
4. Build Windows user-mode Audio Lab without driver.
5. Connect application-level PCM path.
6. Provision driver VM.
7. Begin sample-driver spike only after previous evidence is stable.
