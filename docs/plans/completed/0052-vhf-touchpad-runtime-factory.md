# CAPY-PTP-003E — Runtime-admitted VHF Sink factory

Status: complete

Owner: Codex

Created: 2026-08-31

Requirements: `FR-SCEN-006`, `FR-PLAT-004`, `NFR-STAB-001..004`,
`NFR-SEC-001..003`, `NFR-MAINT-001..003`

## Objective

Connect the VHF touchpad session to the existing Adapter Sink-factory boundary
so the protected driver interface can be opened only after current Runtime
Route and semantic preflight succeeds.

## Completed scope

- implement the Adapter Sink trait for transport-generic VHF sessions;
- add a side-effect-free Windows VHF factory;
- derive the Broker generation from the admitted non-zero Stream epoch;
- release active driver state and rebind Hello on Route epoch advance;
- prove invalid Route preflight returns before VHF interface open;
- add an exact-name ignored real-interface Hello/Close acceptance test;
- reconnect and inspect the authorized Android wireless-debug target.

## Out of scope

- driver installation/signing or invocation of the ignored acceptance;
- production transport selection and Android-to-VHF process composition;
- physical three-/four-finger gesture acceptance.

Completed evidence: `docs/CAPY_PTP_003E_REPORT.md`.
