# ADR 0024: Route lifecycle completions enter through Runtime commands

- Status: Accepted
- Date: 2026-08-24

## Context

Core already enforces the complete Route state machine, but the foundation
Runtime exposed only a deterministic demo helper that authorizes, prepares,
starts and activates a Route in one synchronous call. A real Adapter connects
and stops asynchronously. Letting a Tauri or platform callback mutate Core
state directly would bypass Runtime event sequencing, Problem retention and
epoch rules.

## Decision

Expose small deterministic Runtime commands for Route authorization,
preparation, start intent, successful activation, stop intent, successful stop,
offline recovery and retryable offline Problem reporting. Each successful
mutation emits a bounded structured Runtime event. Runtime revalidates current
catalog endpoints before preparation, start or recovery.

Adapter/platform workers still own I/O. They return typed outcomes to their
host, which acquires Runtime ownership and invokes one of these commands. The
Runtime contains no socket, Tauri, platform SDK or reconnect loop.

An offline Problem must explicitly reference its affected Route. Runtime
validates and retains the Problem, attaches its ID to the Route, invalidates the
current epoch and emits `ProblemReported` plus `RouteChanged(Offline)`. Retry is
an explicit `recover -> begin_start` sequence and therefore advances to a fresh
epoch.

The existing one-step `set_route_active` method remains labelled as a synthetic
Mock/Demo helper; production Adapter integration must use the staged commands.

Runtime also exposes an atomic Adapter-plus-initial-catalog registration seam.
The desktop physical lab uses it to add the SensorServer and built-in Panel
Adapters to the same Node Runtime that already owns the desktop catalog and
Session; it does not create a second UI-owned Runtime.

## Alternatives

Mutate a cloned Core Route in Tauri; reinterpret UI status strings as Runtime
state; put network I/O inside `capyio-runtime`; force every integration through
the Sidecar Host operation registry; expand the Node-to-Node protocol before a
local completion seam is proven.

## Consequences

Physical and future platform integrations can share one deterministic lifecycle
and diagnostic path without adding I/O to Runtime. The command surface is more
verbose than the demo helper, but intermediate states and failure epochs are
observable and testable. Authorization policy and automatic retry scheduling
remain separate later work.
