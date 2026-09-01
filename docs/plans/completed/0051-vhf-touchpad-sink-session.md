# CAPY-PTP-003D — VHF touchpad Sink session

Status: complete

Owner: Codex

Created: 2026-08-31

Requirements: `FR-SCEN-006`, `FR-PLAT-004`, `NFR-STAB-001..004`,
`NFR-SEC-001..003`, `NFR-MAINT-001..003`

## Objective

Compose validated direction-neutral touchpad frames, the bounded VHF snapshot
projector and one exclusive Broker client into a fail-closed platform Sink
session callable by a future Runtime-admitted Windows composition layer.

## Completed scope

- validate the descriptor before any Broker transport is opened;
- perform one Hello handshake and submit one complete snapshot per frame;
- expose explicit Active, Failed and Closed lifecycle states;
- make uncertain delivery terminal and reject later submission or close;
- issue the driver's bounded Close release on explicit close and active Drop;
- provide a direct `open_win32` composition helper without adding a CLI path;
- cover normal, failure, inactive and abandoned-session behavior with a fake
  transport.

## Out of scope

- Runtime Route/factory integration and Android receiver wiring;
- driver installation, device creation, signing or real IOCTL submission;
- three-/four-finger Windows acceptance or PTP certification.

Completed evidence: `docs/CAPY_PTP_003D_REPORT.md`.
