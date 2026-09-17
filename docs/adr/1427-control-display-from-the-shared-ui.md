# ADR 1427: Control the Windows display host from the shared UI

Status: accepted for local source integration; deployment qualification pending

Date: 2026-09-17

## Decision

An optional administrator-owned `--display-control-user-sid` accompanies the
existing absolute `--single-display-config`. With that option, the service opens
a dedicated local `CapyIO.Display.Control.v1` pipe and stays stopped until an
explicit Start. Without it the ADR 1426 opt-in startup behavior is preserved and
no display control pipe is exposed. Existing audio control behavior is unchanged.

The pipe reuses the bounded 4 KiB framing and Windows-validated requester SID
ACL used by camera control, granting System, administrators and the configured
SID, not all interactive users. The administrator must select the intended user
principal; this is not automatic identity enrollment. First-instance creation
rejects an already occupied pipe name. Remote clients are rejected. This is local
OS access control, not authentication of a remote display peer or a complete
server-identity attestation mechanism.

The closed schema accepts only version, request ID and Status/Start/Stop. No
client can choose a file, executable, certificate, network endpoint or Windows
session. Responses contain bounded execution state/generation/problem only.
Start is idempotent while starting/running; stopping rejects a replacement until
the old host is reaped. Stop requests cancellation without waiting for native
cleanup, so a pipe request never joins the live media thread. The service keeps
polling independently of UI requests. Losing the display control listener stops
only its owned display execution, not audio.

Tauri exposes three parameter-free commands on a blocking-task executor rather
than the window thread. The shared CapyIOApi has matching Android/mock behavior
(no Windows sender on those backends). Vue renders one display section with
platform-supported send/receive controls, truthful unavailability and bounded
state-based actions. It polls active transitions without overlapping requests
and stops its timer on unmount without stopping the backend.

`running` means the host is open, not that authentication or media presentation
succeeded. The UI explicitly preserves this distinction. Detailed media counters,
ordinary-user enrollment and unified-device display acceptance remain open.

## Verification

Service tests exercise a real requester-scoped test pipe with missing config,
explicit Start, stable configuration failure and clean listener shutdown. They
also reject duplicate listener ownership, unknown fields/operations, injected
launch authority, invalid correlation and private/unbounded diagnostic strings.
A blocked fake cleanup proves Stop returns Stopping before cleanup completes.
UI contract tests cover all six display states and reject malformed DTOs.

Normal tests do not launch a real display worker, alter topology, deploy a
service, install a driver/APK, issue credentials or change system policy.
No new production dependency or public network protocol is introduced.
