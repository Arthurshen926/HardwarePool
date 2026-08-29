# ADR 0038: Isolate process-owned TCP presence in a platform helper

Status: accepted

## Context

The Audio Share and MicYou external-process Adapters both need a bounded signal
that distinguishes a ready local listener from a connected mobile peer. Parsing
localized human logs would create an unstable lifecycle contract, and a probe
connection proves only listener readiness. On Windows, the IP Helper owner-PID
table can associate established TCP rows with the supervised process and local
port, but reading that table requires platform FFI and unsafe memory access.

The MicYou Adapter deliberately forbids unsafe code. Core and Runtime must not
gain Windows APIs or peer-table layouts merely to observe an Adapter-owned
private transport.

## Decision

Create the small Apache-2.0 `capyio-process-presence` platform crate. It owns the
reviewed `GetExtendedTcpTable` calls, a 16 MiB response bound and table-layout
validation. Its safe API accepts only process ID plus typed local socket address
and returns `UnsupportedPlatform`, `Disconnected` or an established connection
count. It does not return or retain peer addresses.

Audio Share and MicYou map that neutral result into their own Adapter-specific
status and error types. The platform signal proves transport presence only. A
host policy requires consecutive observations before activating a Runtime Route;
it does not infer protocol negotiation, decoded PCM, audibility, Android
permission state or microphone quality.

Non-Windows builds return `UnsupportedPlatform` without attempting a fallback
shell command or log parser.

## Consequences

- unsafe Windows table access is centralized outside Core, Runtime and the two
  Adapters;
- both audio directions use the same bounded platform observation without
  creating an Audio Share-to-MicYou dependency;
- `windows-sys` 0.61.2 remains the reviewed MIT OR Apache-2.0 Windows binding;
- polling cadence, stable-count threshold, timeout, Route transition and Problem
  mapping remain host/Adapter policy rather than platform-helper behavior;
- a future non-Windows implementation requires its own review and tests.
