# ADR 1514: Bounded Linux headless Node host

Status: accepted for implementation, 2026-10-19.

## Context

Linux previously had no platform host. The existing CLI demonstrates synthetic
Windows/Android Nodes; running it on Linux does not make those Linux devices.
The Windows local Node API uses Windows-specific process and named-pipe controls.
Adapter JSON-RPC is a different boundary with initialization and Route semantics;
a Node management endpoint must not impersonate an Adapter.

## Decision

Add `capyio-linux-host` as a thin owner of the existing deterministic NodeRuntime,
and `capyio-node linux-host --node-id <UUID> [--name <name>]` as its Linux-only
entrypoint. No new external dependency is introduced: reuse workspace serde,
serde_json, Core and Runtime. The caller explicitly supplies a non-nil stable ID;
no credentials or persistent identity files are generated. Names are nonempty,
control-character-free and at most 128 UTF-8 bytes.

Inherited stdin/stdout carry a separate versioned local NDJSON management API.
Each request has exactly `version` (1), `id` (u64) and `command` (string).
Commands are `get_snapshot`, `host_status` and `host_shutdown`. Responses carry
`version`, `id` and either `result` or `error.code`. Unparseable requests have a
null correlation ID. Unknown fields, versions and commands fail closed. Request
records are bounded to 4096 bytes excluding LF; responses to 65536 bytes excluding
LF. Complete malformed records permit retry. Oversized records emit one bounded
error and terminate without draining an indefinitely unterminated input. EOF
accepts a final complete JSON record then exits; explicit shutdown acknowledges
and ignores subsequent queued records. Logs use stderr exclusively.

The host owns one Runtime until exit. Its catalog is honestly empty: no phantom
hardware, synthetic grants, peers or Sessions. No mutable Runtime handle escapes
this composition. The only lifecycle mutation is local host termination, not a
Runtime domain mutation. No socket, filesystem persistence, OS permission change,
service installation, credential or physical-device access occurs.

## Consequences and limits

This is a functioning headless management process, not a system daemon or UI
integration. Its owner controls inherited pipes. There is no peer authentication,
remote-client timeout promise or Windows local API compatibility claim. Stdio
backpressure can block the process; input is processed sequentially without an
unbounded queue. Since hardware/Route workers do not yet exist, cleanup means
releasing Runtime and pipes. Adding workers requires explicit stop/join tests.

Mock Sidecar Route composition is deferred, not claimed as tested here. A future
slice should reuse the existing portable Adapter host and explicit synthetic
catalog, rather than grant production authorizations or publish fake Linux audio.
Network control, protected identity storage, PipeWire, V4L2, uinput, systemd and
native Tauri packaging remain separately scoped work.

## Validation

Library tests cover identity, snapshot consistency, strict schema/error recovery,
byte bounds, EOF, invalid UTF-8 and shutdown. Actual Linux subprocess tests cover
multiple requests in one PID, clean EOF/shutdown and oversized-input termination
while stdin remains open. Every process test has response/exit deadlines and
kills/reaps its child on failure. These checks do not establish hardware support.
