# ADR 1474: Explicit publication owns the Android waiting service

Status: accepted for the active connection-reliability slice

Date: 2026-09-18

## Observed mismatch

The architecture requires UI-independent Runtime ownership. On the Y700, with
no active hardware, leaving MainActivity unbound the last client and destroyed
AudioNodeService, including the control listener. The app process survived;
returning recreated the service and changed its listening port. This violates
the expected published-node waiting lifecycle. It does not establish the cause
of Android wireless-debugging port changes.

## Decision

An explicit local Source publication selection starts the existing connected
device foreground Service. Waiting for an authenticated peer is visible through
the existing ongoing notification. Publication does not authorize a Route or
start any hardware. Removing the last publication relinquishes this ownership;
active hardware retains its existing independent foreground ownership. The
notification Stop All action also relinquishes publication ownership. There is
no boot start, persisted restart, permission bypass or new manifest declaration.
Existing connectedDevice type and CHANGE_NETWORK_STATE permission are reused.

The empty, unshared Node remains Activity-bound in this slice. A future explicit
whole-node switch may replace publication intent as the waiting owner. This
does not claim lock-screen/oem battery-policy survival without device evidence.

## Verification

Extend pure ownership policy tests for idle publication and no-owner release.
On the authorized Y700 enable one Source, leave the app, verify the same native
control port and mTLS catalog connection, return and disable publication. Verify
no capture/input Route started. No dependency is added.
