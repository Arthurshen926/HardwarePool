# CAPY-PTP-003B — Bounded Broker-to-VHF submission

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-PLAT-004`, `NFR-STAB-001..004`,
`NFR-SEC-001..003`, `NFR-PERF-001..003`, `NFR-MAINT-001..003`

## Objective

Extend the compile-only VHF source driver with a least-privilege, exclusive,
fixed-size local Broker ABI and bounded complete-snapshot-to-hybrid-HID report
submission. Make no host deployment or persistent system change.

## In scope

- LocalSystem/Administrators-only device security and one exclusive open;
- canonical fixed-size Hello/Data/Ack/Close records;
- strict version, size, sequence, contact-ID, coordinate and flag validation;
- at most five active contacts and fixed preallocated previous-contact state;
- explicit lift reports at last known coordinates and bounded replacement
  splitting;
- synchronous `VhfReadReportSubmit` status propagation and poisoned-session
  behavior after uncertain partial submission;
- Microsoft's documented default pre-certification 256-byte PTPHQA blob;
- WDK compile, InfVerif, hardware-free validation and full repository CI.

## Out of scope

- driver installation/removal, root-device creation, signing or certificates;
- user-mode Broker process/service integration and Android/network input;
- retries after a possibly partial VHF frame;
- claiming Windows enumeration or three-/four-finger compatibility.

## Acceptance criteria

1. Non-administrator device access is denied by construction and only one file
   session can be open.
2. Malformed, noncanonical, out-of-order or over-five-contact records never
   reach VHF.
3. A complete active-contact snapshot produces bounded hybrid reports;
   disappeared contacts are released at their last coordinates.
4. Submission failure poisons the file session and no later Data is accepted.
5. The unsigned x64 driver rebuilds, its INF and static validators pass, and no
   deployment/signing command runs.

## Safety

Compile and static validation only. Do not call `pnputil`, `devcon`, `sc`,
driver deployment/signing tools, `bcdedit`, Verifier or boot/security APIs.

Completed evidence: `docs/CAPY_PTP_003B_REPORT.md`.
