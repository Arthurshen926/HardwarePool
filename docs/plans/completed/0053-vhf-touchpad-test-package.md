# CAPY-PTP-003F — VHF local-lab test package

Status: complete

Owner: Codex

Created: 2026-08-31

Requirements: `FR-SCEN-006`, `FR-PLAT-004`, `NFR-SEC-003`,
`NFR-STAB-001`, `NFR-MAINT-001..003`

## Objective

Generate an exact, locally test-signed VHF driver package and scoped rollback
procedure without installing the package, trusting its certificate or changing
Windows boot/security policy.

## Completed scope

- hash-pin the input INF and unsigned SYS;
- sign the staged SYS, run Inf2Cat, sign the resulting catalog;
- export only the public test certificate and delete the temporary private key;
- verify the certificate was not left in inspected personal/root/publisher stores;
- record exact staged artifact hashes and signing thumbprint;
- add a fail-closed rollback script scoped to CapyIO identifiers only;
- capture the ADR 0029 elevated recovery inventory through explicit UAC.

## Out of scope

- trusting the test certificate, enabling test signing or changing Secure Boot;
- installing/removing the driver, creating a root device or invoking VHF;
- claiming deployment readiness before independent recovery access and boot
  policy approval are confirmed.

Completed evidence: `docs/CAPY_PTP_003F_REPORT.md`.
