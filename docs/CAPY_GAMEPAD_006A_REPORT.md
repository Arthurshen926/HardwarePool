# CAPY-GAMEPAD-006A — Windows virtual Xbox 360 lab record

Status: protocol gate passed and signed USB/IP client deployed; the installer
requires a Windows restart before device attachment and final enumeration.

Date: 2026-08-30

## Scope and safety boundary

This slice turns the existing Android touch + IMU source into two concurrent
projections:

- DSU v1001 on explicit IPv4 loopback for motion-aware consumers;
- a VIIPER-owned Xbox 360 bus/device/stream for Windows USB/IP attachment.

The lab command is debug-only, accepts only an explicit loopback VIIPER API
port, bounds its hold time to 5–300 seconds, starts neutral, neutralizes an
Android peer timeout after 350 ms and explicitly removes its owned VIIPER bus.
It does not modify Secure Boot, BitLocker, test signing, boot configuration or
Driver Verifier.

## Pinned host and packages

- authorized host: `DESKTOP-AT8EVE9`, Windows build `10.0.26200.0`, x64;
- VIIPER: upstream v0.7.0, release commit `6b71b14`;
- VIIPER Windows archive SHA-256:
  `a02b06751d64e43e7700aba8ee1f7e3e4f5f4e7f370a11722ff922ab075c1629`;
- USB/IP client: usbip-win2 v0.9.7.7 x64 (v0.9.7.8 is intentionally excluded
  because its release carries an upstream memory-corruption/BSOD warning);
- USB/IP installer SHA-256:
  `51620fa5f9f8be5932bc9d786deee557ce06d5407a99cab490dcfac71f185fea`;
- installer Authenticode: `Valid`, signer `Cloudyne Systems (Scheibling
  Consulting AB)`, certificate thumbprint
  `9AC56B6C76141395D74FFF6652818376E80B9C95`, Microsoft timestamp;
- expected root device: `ROOT\USBIP_WIN2\UDE`;
- expected services/drivers: `usbip2_ude` and `usbip2_filter`;
- expected application directory: `C:\Program Files\USBip`.

The signed installer is invoked with Inno Setup's silent, no-restart switches.
If it reports that a reboot is required, the gate stops before reboot and asks
the operator separately. The upstream installer restarts USB 3.0 hubs, so USB
devices can disconnect briefly during installation.

## Rollback

Preferred rollback is the installed USBip uninstaller in Windows Apps. The
upstream fallback, run elevated only after resolving exact targets, is:

```text
C:\Program Files\USBip\devnode.exe remove ROOT\USBIP_WIN2\UDE root
pnputil.exe /remove-device /deviceid ROOT\USBIP_WIN2\UDE /subtree
```

Then locate only the published `oem*.inf` packages containing both
`usbip2_filter` and `usbip2_ude`, delete those exact packages with
`pnputil.exe /delete-driver <resolved-oem.inf> /uninstall`, and remove only the
resolved `C:\Program Files\USBip` application directory. This task does not
enable test signing, so rollback must not change boot policy.

The exact published packages resolved on this host are `oem171.inf`
(`usbip2_filter.inf`, attested Extension driver) and `oem172.inf`
(`usbip2_ude.inf`, attested USB driver). These names are evidence for this
installation only; rollback must re-resolve them instead of assuming they stay
stable.

## Authorized driver deployment evidence

- a system restore point named `CapyIO USBip preinstall 2026-08-30` was
  created before installation;
- the pinned installer exited `0` under an elevated administrator token;
- the embedded VC++ runtime reported `1638` (a compatible/newer runtime was
  already present);
- `pnputil` installed the hub filter and returned `3010` (restart required);
- the UDE root-device installation returned `0`;
- the installer recorded `Need to restart Windows? Yes` and `Will not restart
  Windows automatically`; no restart has been performed by this task;
- both `usbip2_filter` and `usbip2_ude` currently report `RUNNING` and
  `DEMAND_START`;
- the root controller currently reports `OK` as `ROOT\USB\0001`, with hardware
  ID `ROOT\USBIP_WIN2\UDE`, service `usbip2_ude`, and `oem172.inf`;
- `C:\Program Files\USBip\usbip.exe --version` reports `0.9.7.7`;
- the registered quiet uninstaller is
  `"C:\Program Files\USBip\unins000.exe" /SILENT`.

No VIIPER device has been attached to the Windows USB/IP client yet. The
restart required by the signed installer is the current gate boundary.

## Real VIIPER pre-deployment evidence

The portable server was started with both API and USB endpoints bound to
`127.0.0.1`, and with local-client and Windows-native automatic attachment
explicitly disabled. The pinned adapter received:

```json
{"server":"VIIPER","version":"0.7.0"}
```

The new gate then exercised the real server using a bounded synthetic Android
source:

```text
accepted=40
rejected=0
replayed=0
peer_timeouts=1
viiper_states=41
non_neutral_controls=true
finite_imu=true
owned_bus=1
owned_device=1
gate=passed
```

The server log shows `ping -> bus/create -> bus/1/add -> bus/1/1`, an initial
neutral stream frame, the accepted states, peer-disconnect neutralization and
`bus/remove`. The retained aggregate log is
`target/evidence/gamepad-006a/synthetic-viiper-gate.stdout.log`; ephemeral
pairing credentials and the VIIPER local authentication key are intentionally
excluded from this report.

## Debug gate

```powershell
target/debug/capyio-desktop.exe --gamepad-viiper-physical-gate 31581 26761 3242 90
```

The gate prints the fresh Android pairing token plus the exact VIIPER bus and
device IDs. A separate elevated USB/IP client operation must attach the exact
exported bus ID before Windows can enumerate the Xbox 360 controller.
