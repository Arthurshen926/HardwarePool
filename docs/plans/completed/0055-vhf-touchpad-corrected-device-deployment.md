# CAPY-PTP-003I — corrected VHF device deployment

Status: complete

Owner: Codex

Created: 2026-08-31

Requirements: `FR-SCEN-006`, `FR-PLAT-004`, `NFR-SEC-003`,
`NFR-STAB-001`, `NFR-MAINT-001..003`

## Objective

Install the exact approved 003H package without restarting Windows and prove
that the protected Broker interface can complete Hello/Close without input.

## Completed scope

- verified exact package hashes and certificate identity;
- installed the root source device without a restart request;
- confirmed running PnP device, `vhf` lower filter and kernel service;
- enumerated exactly one protected Broker interface;
- passed the exact elevated Hello/Close acceptance with zero submitted frames;
- retained the exact, version-scoped rollback command.

## Remaining gate

Desktop input submission remains a separately visible acceptance action. The
next slice should add and compile an exact VHF one-finger lifecycle test, then
run it only under the existing explicit desktop-input authorization boundary.

Evidence: `docs/CAPY_PTP_003I_REPORT.md`.
