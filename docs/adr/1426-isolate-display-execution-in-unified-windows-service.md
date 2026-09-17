# ADR 1426: Isolate display execution in the unified Windows service

Status: accepted for opt-in integration; product control and qualification pending

Date: 2026-09-17

## Context

The historical Display main loop shares a service-wide cancellation flag and
ends its service loop after display-thread exit. Importing it unchanged would
conflict with independent audio/display failure semantics. ADR 1422 requires
composition with the newer audio service, not replacement of that service.

## Decision

Add an optional administrator-owned `--single-display-config <absolute-path>`
to the existing service executable, including its bounded console mode. The
default command and existing audio control pipe remain unchanged. No UI or
pipe request supplies the path, executable, credentials or endpoints.

`DisplayServiceRuntime` owns one host thread and its own cancellation flag.
Configuration loading and host construction occur on that thread, keeping
thread-affine Windows mappings and COM objects there. The service's ordinary
poll loop observes only a bounded execution snapshot and never joins a live
display thread while polling audio. Opening, running, cancellation and failure
are separate states; `running` means host-open, not authenticated media delivery.
Unexpected successful exit is a failure, not a streaming claim. There is no
automatic restart; a replacement cannot begin before the old host is reaped.

Startup observation has a fixed ten-second deadline. Expiry requests display
cancellation without ending the service, retains the timeout and prevents an
overlapping replacement. Service shutdown requests both lifecycles to stop and
attempts their cleanups even when one fails. Diagnostics expose static problem
codes and generation/state only; they contain no configuration path or secrets.

Shutdown joins the cooperative host instead of abandoning it. The inherited
engine-worker cleanup allows 45 seconds for graceful topology restoration.
This is not a hard bound on a hung in-process native API: process-level watchdog
and hostile/frozen-host shutdown qualification remain release work. The SCM
stop hint is 60 seconds, not proof of a guaranteed shutdown deadline.

## Validation and limits

Hardware-free tests cover independent audio state after display failure,
duplicate start, generation advance, failed open, unexpected exit, panic,
cancellation during open, startup timeout, thread-affine resources and Drop
cleanup. Existing audio and display worker tests remain required.

The optional host is not enabled on the installed service by this source change.
No driver, service configuration, credentials, APK or physical topology changes
are part of this gate. Per-display local control/UI, detailed media status,
credential enrollment and unified-device media acceptance remain separate
unfinished work. No new dependency or public network protocol is introduced.
