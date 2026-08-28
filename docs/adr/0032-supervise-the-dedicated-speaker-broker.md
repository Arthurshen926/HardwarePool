# ADR 0032: Supervise the dedicated speaker Broker as a fixed projection

Status: accepted

Supersedes: the `CAPY-AUDIO-001B4` assumption that the existing Audio Share
playback-endpoint picker should select the dedicated virtual speaker.

## Context

The original Audio Share Quick Action supervises the pinned upstream `as-cmd`
process. That process enumerates a Windows playback endpoint, captures its
loopback stream and owns the upstream private transport. The dedicated
`CapyIO Speaker` path selected by ADRs 0027–0031 is different: its fixed MFX
copies the endpoint's post-mix PCM into the bounded render ring, and the
CapyIO-owned `capyio-virtual-speaker` Broker consumes that ring and owns the
Android transport.

Making the old endpoint picker launch the new Broker would be misleading. The
new Broker has no arbitrary endpoint input and must never capture the current
physical/RDP endpoint. Running both processes on the same bind address would
also conflict.

## Decision

The trusted desktop host accepts a separate
`CAPYIO_VIRTUAL_SPEAKER_EXE` configuration. When it is present, the Runtime
supervises that executable directly with one positional, explicit IPv4 bind
address. It skips the upstream version/endpoint probe, hides the endpoint
picker and describes the Route as the fixed `CapyIO Speaker` projection.

The existing `CAPYIO_AUDIO_SHARE_EXE` and playback-endpoint configuration stay
available as the legacy system-mirror mode. They are not silently reinterpreted.
The WebView cannot supply either executable path, bind address or raw endpoint
identifier.

Both modes retain the same bounded child-process lifecycle, owner-scoped TCP
presence observation, Route state machine and explicit start/retry/stop
commands. Long-term service separation remains follow-up work; this slice does
not claim that closing the current desktop host preserves an active Route.

## Consequences

- `CapyIO Speaker` is a fixed Projection rather than a user-selectable capture
  source.
- The normal desktop start/stop path now owns and reaps the dedicated Broker;
  a manually orphaned lab process is no longer required.
- The legacy system-audio mirror remains usable and clearly labeled as a
  different mode.
- Broker/receiver loss remains fail-silent and produces the existing typed
  Route diagnostics. Automatic retry and Windows-service persistence are not
  introduced by this decision.
- Physical validation must start the dedicated Broker through this supervised
  path and prove Android receiver presence and audible playback.
