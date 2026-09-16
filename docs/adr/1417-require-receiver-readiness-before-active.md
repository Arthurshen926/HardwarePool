# ADR 1417: Require receiver readiness before Active

Status: accepted

Date: 2026-09-16

## Decision

Desktop-managed Android camera and touchpad receivers no longer become Active
merely because Windows created a child process. Their stdout is a fixed
readiness control channel:

- camera receivers require a bounded UTF-8 line beginning exactly with
  `CAPYIO_AVC_LAB_READY `;
- the touchpad receiver requires exactly `lab_status=listening`.

The desktop waits at most five seconds and reads at most 512 bytes. It accepts
LF or CRLF, then drains later stdout without retaining it. stderr remains on
the existing 4 KiB tail. Mismatched/oversized records, EOF, timeout or channel
failure terminate and reap the child, remove the exact ADB reverse and return a
bounded failure. No readiness contents or executable arguments originate in
the WebView.

This readiness means the fixed loopback listener and local projection setup
completed; it does not claim that a phone connected or that media/input frames
arrived. Those remain separate physical evidence levels.

## Evidence

Unit coverage accepts each kind's exact record and rejects cross-kind,
oversized and malformed records. Strict desktop Clippy passes. On the
authorized Android debugging device, the decode-only camera action received its
real receiver readiness, became Active, stopped, and left no `tcp:38173` ADB
reverse. The phone camera was not started and no pixels were retained in this
specific readiness test.
