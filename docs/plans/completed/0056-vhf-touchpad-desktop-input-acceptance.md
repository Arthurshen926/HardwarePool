# CAPY-PTP-003J..003N — VHF desktop input acceptance

Status: completed

## Goal

Submit bounded fixed one-, two-, three- and four-contact fixtures through the
installed protected VHF interface and record objective Windows desktop effects,
without installing/removing a driver or restarting Windows.

## Acceptance criteria

- One-finger motion changes system cursor coordinates and produces no click.
- A one-finger tap produces exactly one isolated-target click.
- A two-finger pan produces a foreground wheel event.
- Three- and four-finger horizontal swipes each produce an observable Windows
  Shell/foreground state reaction.
- Every fixture releases contacts, closes the Broker generation and retains an
  elevated transcript.
- Test scripts pin the exact acceptance executable and remain ignored by
  default CI.

## Result

All criteria passed. Cursor delta was `(+106,+8)`, tap count was `1`, wheel
event count was `1` with delta `582`, and both Shell gesture targets observed a
deactivation. The four-finger run cannot record a virtual-desktop GUID because
that registry value is absent in the current profile; it instead records the
foreground target's independent activation/deactivation telemetry.

Evidence: `docs/CAPY_PTP_003J_REPORT.md`.
