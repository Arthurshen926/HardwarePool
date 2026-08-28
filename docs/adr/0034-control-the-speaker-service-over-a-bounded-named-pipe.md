# ADR 0034: Control the speaker service over a bounded named pipe

Status: accepted

Amends: ADR 0033's initial no-management-IPC service slice.

## Context

The `CapyIOBroker` service must own the Session 0 render ring, but the ordinary
desktop user must be able to start, stop and observe speaker sharing without
elevating Tauri. Starting a second Broker in the desktop would create competing
ring and port owners, while making window lifetime own the Route would stop
headless playback when the UI closes.

## Decision

Expose one local byte-mode named pipe,
`\\.\pipe\CapyIO.Broker.Control.v1`, from the Windows service. Messages use a
four-byte little-endian length followed by closed schema-v1 JSON. Requests are
limited to 4 KiB and the finite `status`, `start` and `stop` operations;
responses contain only the request ID, success, a bounded typed snapshot and a
stable problem code. Client and server I/O have fixed deadlines.

The pipe rejects remote clients. Its protected DACL grants full access to
LocalSystem and Administrators and explicit read/write access to interactive
local users. It does not accept a process path, bind address, port, endpoint ID
or arbitrary arguments. Those values remain administrator-owned service launch
configuration. The service serializes requests through one Runtime owner and
reuses one disconnected pipe instance, so repeated queries cannot strand the
only instance.

CapyIO Desktop probes this boundary first and projects the service state into
the existing Runtime Route and Quick Action. Explicit Quick Action start/stop
controls the service-owned Broker. Closing the desktop does not stop it. The
environment-configured direct process remains a development fallback only when
the service pipe is absent.

## Consequences

- Daily start/stop/status operation requires no administrator token after the
  service has been installed and configured.
- Any interactive local user can control this machine-wide speaker Route. This
  is acceptable for the current single-user lab but requires a per-user policy
  decision before multi-user production deployment.
- The pipe authenticates a local Windows logon class, not an Android peer.
  Audio Share's private TCP/UDP transport still has no CapyIO pairing or
  application-layer encryption and remains restricted to the trusted lab.
- Service installation, binary/configuration updates and removal remain
  administrator operations. A distributable installer is still release work.
