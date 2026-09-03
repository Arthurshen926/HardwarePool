# CAPY-PTP-003A — VHF Precision Touchpad compile baseline

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-PLAT-004`, `NFR-STAB-001..004`,
`NFR-SEC-001..003`, `NFR-MAINT-001..003`

## Objective

Create a compile-only KMDF/VHF Precision Touchpad source-driver baseline with a
byte-exact mandatory HID descriptor and no deployment or persistent host
change.

## In scope

- mandatory Precision Touchpad and Configuration HID top-level collections;
- five-contact hybrid capability, fixed-size contact input report and required
  feature reports;
- minimal VHF create/start/delete lifecycle;
- bounded local driver-control ABI skeleton for a future user-mode Broker;
- x64 WDK build, InfVerif/static validation and hardware-free descriptor tests;
- exact package inventory and hashes after a successful build.

## Out of scope

- driver install/remove, root-device creation or Device Manager enumeration;
- signing certificates, test signing, Secure Boot, BitLocker, boot settings,
  Verifier, reboot or production packaging;
- Android/network parsing, reconnect, gesture recognition or Windows policy in
  kernel mode;
- claiming three-/four-finger compatibility before a separately approved
  deployment Gate.

## Acceptance criteria

1. The descriptor exposes the mandatory touchpad and Configuration collections
   and supports exactly five contacts.
2. Report structures are fixed-size and compile-time bounded.
3. Driver entry creates one VHF device and teardown deletes it without placing
   untrusted/session logic in kernel mode.
4. The package builds with the installed WDK and passes available INF/static
   validation without installation.
5. Tests and documentation record exact commands, artifacts and remaining
   deployment risk.

## Required evidence

```text
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

Driver build and validation commands will be recorded after resolving the
installed Visual Studio/WDK project toolchain.

Completed evidence: `docs/CAPY_PTP_003A_REPORT.md`.

## Safety

This slice is compile-only. It must not call `pnputil`, `devcon`, `sc`, driver
deployment/signing tools, `bcdedit`, Verifier or any boot/security-policy API.
