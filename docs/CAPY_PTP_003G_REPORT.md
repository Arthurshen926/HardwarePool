# CAPY-PTP-003G Report — first controlled VHF deployment attempt

Date: 2026-08-31

Status: failed safely; fully rolled back without restart

## Outcome

The exact 003F package was trusted and installed under the separately approved
local-lab exception. DevCon created `ROOT\SYSTEM\0001` without requesting a
restart, but the source device failed PnP start with problem 31. SetupAPI
recorded `CM_PROB_FAILED_ADD` and status `0xc0000010`.

The package, root device, service and both copies of the 003F certificate were
then removed. DevCon and PnP inventory no longer find the device, `oem175.inf`
is absent, and Windows was not restarted.

## Root cause

`VhfCreate` requires the Microsoft VHF source driver (`vhf.sys`) to be attached
as a lower filter. The 0.0.1.0 INF had no `LowerFilters` registration, so the
KMDF source function driver could not create its VHF handle during device add.

The corrected INF adds the device hardware section and the exact `vhf` lower
filter registration. No network, pairing or packet parsing logic moved into the
kernel driver.

## Retained evidence

- install transcript: `target/lab-evidence/CAPY-PTP-003G-install.txt`;
- rollback transcript: `target/lab-evidence/CAPY-PTP-003F-rollback.txt`;
- SetupAPI status: problem `0x1f`, status `0xc0000010`;
- exact failed package manifest: `docs/CAPY_PTP_003F_REPORT.md`.

This attempt does not count as an enumerated Precision Touchpad or desktop
input result.
