# CAPY-PTP-003G/003H — first deployment and VHF attachment correction

Status: complete

Owner: Codex

Created: 2026-08-31

Requirements: `FR-SCEN-006`, `FR-PLAT-004`, `NFR-SEC-003`,
`NFR-STAB-001`, `NFR-MAINT-001..003`

## Objective

Perform one controlled, no-restart deployment of the exact approved package;
if it cannot start, retain evidence, roll it back and prepare a corrected exact
package without claiming successful enumeration.

## Completed scope

- installed the exact 003F package without a restart request;
- retained PnP and SetupAPI failure evidence;
- removed the failed device, package, service and trusted certificates;
- corrected the missing VHF lower-filter INF registration;
- rebuilt with code analysis and validated the descriptor and INF;
- generated an exact 003H signed package and fail-closed install/rollback pair.

## Remaining gate

The materially changed 003H package must receive exact-package deployment
approval before certificate trust, root-device creation or desktop input tests.

Evidence: `docs/CAPY_PTP_003G_REPORT.md` and
`docs/CAPY_PTP_003H_REPORT.md`.
