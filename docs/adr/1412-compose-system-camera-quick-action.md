# ADR 1412: Compose a system-camera Quick Action

Status: accepted

Date: 2026-09-16

## Decision

Expose a separate fixed `android-system-camera` desktop Quick Action. Preserve
ADR 1409's decode-only action; the two actions are mutually exclusive because
they use the same loopback camera transport. The system action resolves its
fixed sibling executables and one authorized ADB device, starts ADR 1411's
camera session host, requires its exact readiness line within ten seconds,
then starts the existing receiver with `--publish-shared`. It retains the
7200-access-unit and 60-second reconnect bounds. No executable, arguments,
camera identity, registry path or mapping name comes from UI input.

Stop closes the camera host's stdin and waits up to three seconds for normal
Stop/Shutdown before terminating it and reporting failure. Receiver shutdown
and exact ADB-reverse cleanup are attempted even if camera shutdown fails.
Failure of either process stops the other at the next action-state poll.
Camera readiness means session registration, not receipt of a live phone frame;
the existing source can show an offline placeholder before live data arrives.

The package includes the host but does not deploy the COM source. System use
requires the existing separately reviewed COM deployment and permission to
create the fixed Global shared mapping. No driver or COM installation is
performed by Start. Missing dependencies remain visible failures. Physical
ordinary-consumer validation is required before claiming the new action works
end to end on a deployed host.
