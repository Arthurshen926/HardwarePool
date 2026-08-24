# ADR 0026: Observe Audio Share receiver transport through the Windows TCP owner table

- Status: Accepted
- Date: 2026-08-24

## Context

Audio Share v0.3.4 exposes no machine-readable peer-status API. Parsing its
ordinary human logs would violate CapyIO's diagnostic contract, while modifying
or proxying the upstream data plane is outside the active slice. The first
remote-speaker target is Windows, where the documented IP Helper
`GetExtendedTcpTable` API can return TCP state and owning process ID.

## Decision

Use target-specific `windows-sys` 0.61.2 bindings for
`GetExtendedTcpTable(AF_INET, TCP_TABLE_OWNER_PID_ALL)`. Filter only established
rows matching the supervised `as-cmd` process ID and explicit local port. Do not
retain or expose local/remote IP addresses from the table.

The query uses the documented two-call size pattern, a 16 MiB allocation bound,
aligned storage and checked row-count arithmetic. Other platforms report
`UnsupportedPlatform` until an equivalent reviewed implementation exists.

The resulting state is named `ReceiverTcpPresence`, not playback health. An
established row proves only that a TCP peer remains attached to the pinned
server process; it does not prove successful Audio Share negotiation, UDP PCM,
Android `AudioTrack` submission or audibility. Runtime/UI work must preserve
that distinction. A later transition from established to disconnected may be
used as transport-loss evidence in the trusted local lab.

`windows-sys` is maintained by Microsoft under MIT OR Apache-2.0 and is already
present in the locked dependency graph. It is added as a Windows-only direct
dependency with only `IpHelper` and `WinSock` features.

## Alternatives

- parse upstream log prose;
- invoke PowerShell or `netstat` and parse localized text;
- add raw hand-written Windows FFI declarations;
- modify/vendor Audio Share or proxy its TCP/UDP data plane;
- claim TCP-listener readiness as receiver health.

## Consequences

Windows can observe process-owned receiver transport without upstream changes,
shell execution or private-address retention. False peers remain possible in
the current unauthenticated local-lab mode, and established TCP remains a weaker
signal than negotiated/audible playback. Production authorization and peer
identity are still required.
