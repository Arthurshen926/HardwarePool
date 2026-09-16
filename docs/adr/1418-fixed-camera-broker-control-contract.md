# ADR 1418: Fix the least-privilege camera Broker control contract

Status: accepted

Date: 2026-09-16

## Decision

The future elevated Windows camera Broker has a separate, fixed local control
surface at `\\.\pipe\CapyIO.Camera.Control.v1`. It is not added to the existing
audio Broker lifecycle. The pipe rejects remote clients and grants control only
to LocalSystem, administrators and one Windows-validated requesting-user SID.
The SID handoff needed by cross-account UAC is specified by ADR 1420.

Schema version 1 is closed and admits only `status`, `start` and `stop`. Requests
carry a monotonically allocated client request ID; responses must echo both the
version and ID. Neither side accepts an executable, argument, path, mapping
name, camera identity or transport endpoint. Messages retain the shared 4 KiB
bound and existing local-pipe deadlines.

An independent runtime owns one fixed camera process implementation and exposes
only `stopped`, `active` or `failed`, a generation and a stable problem code.
Start and Stop are idempotent. A partial Start is rolled back; rollback failure
has a distinct problem code rather than being reported as stopped. Status moves
an unexpectedly exited Active generation to Failed.

This slice defines and tests the least-privilege boundary only. It does not add
or install a Windows service, launch an elevated process, move the real camera
session/receiver into the Broker, change COM registration, or remove the
desktop system-camera action's existing elevation preflight. Those require a
separate implementation and physical gate.

## Evidence

Windows unit tests cover closed/versioned decoding, bounded authority-free
responses, idempotent lifecycle, partial-start rollback, unexpected exit and a
real requester-scoped named-pipe client/server round trip. Strict Clippy passes
for all `capyio-windows-service` targets.
