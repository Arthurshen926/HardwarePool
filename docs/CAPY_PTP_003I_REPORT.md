# CAPY-PTP-003I Report — corrected VHF device deployment

Date: 2026-08-31

Status: installed and running; no restart requested

## Outcome

The separately authorized, exact 003H 0.0.2.0 package was installed through an
elevated UAC process. Before installation the script revalidated all four
package hashes, certificate thumbprint, existing `testsigning Yes` state and
disabled Secure Boot. It did not change BCD and DevCon did not request a
restart.

Post-install state:

```text
instance: ROOT\SYSTEM\0001
name: CapyIO VHF Precision Touchpad Source
PnP/DevCon state: Driver is running
controlling service: CapyIOVhfTouchpad
lower filter: vhf
service state: RUNNING
service start: DEMAND_START
VHF Broker interfaces: exactly one
```

The package certificate exists only in the intended LocalMachine Root and
TrustedPublisher stores with thumbprint
`D6EF151680FAC70FDF34623DEC23041D76372D7E`.

## Broker acceptance

The exact ignored acceptance executable was hash-pinned to
`56D00FE1B4071AD4F924F7EFECAC8DE1133E3D5ABEF9CAB96CF27CF54BA4D013`
and run elevated. The authorized Runtime VHF factory opened the protected
interface, completed Broker Hello and Close, and submitted zero frames:

```text
1 passed; 0 failed
frames_submitted=0
```

This proves driver enumeration, VHF attachment and the protected user/kernel
control path. It does not yet prove contact submission, mouse-pointer behavior,
Windows Precision Touchpad classification or three-/four-finger Shell actions.

## Evidence and rollback

- install transcript: `target/lab-evidence/CAPY-PTP-003H-install.txt`;
- state transcript: `target/lab-evidence/CAPY-PTP-003G-driver-state.txt`;
- Hello/Close transcript:
  `target/lab-evidence/CAPY-PTP-003H-vhf-open-close.txt`;
- rollback command:
  `scripts/remove_windows_touchpad_003h_test_driver.ps1`.

No restart occurred or was requested.
