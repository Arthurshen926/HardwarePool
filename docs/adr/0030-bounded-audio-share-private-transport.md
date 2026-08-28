# ADR 0030: Implement a bounded Audio Share-compatible Broker transport

- Status: Accepted
- Date: 2026-08-26

## Context

The CapyIO Speaker path selected in ADR 0028 needs the user-mode Broker to send
PCM copied from the render APO staging ring to the existing Audio Share Android
receiver. The pinned Audio Share v0.3.4 command-line server cannot accept PCM
from another process: its network manager is constructed with, and starts, its
own WASAPI loopback capture manager.

Fixed-revision review confirmed that the Android contract is small. TCP uses
little-endian 32-bit commands for format negotiation, playback start and
heartbeat. The format body is one bounded Protobuf message. UDP associates a
32-bit session id and then carries frame-aligned little-endian PCM datagrams.
There are no sequence numbers, authentication, encryption or replay defense.

## Decision

Implement a CapyIO-authored, Audio Share v0.3.4-compatible private transport in
the existing `audio-share` Adapter. It accepts validated `capyio-audio` PCM
format semantics, encodes only the pinned format message and exposes a bounded,
non-blocking Broker-owned PCM queue. A worker divides PCM on sample-frame
boundaries below the upstream IPv4 UDP payload bound. Media never enters the
Sidecar JSON-RPC control path, Core, a driver or the render APO callback.

The server has fixed queue, block and peer bounds; explicit IPv4 binding;
bounded socket waits; heartbeat expiry; deterministic shutdown; and no ordinary
per-frame logging. UDP registration is accepted only for a live session id and
the same source IP as its TCP control session. Queue saturation and malformed
PCM are explicit errors, not silent unbounded buffering.

This remains an `AdapterManaged` compatibility contract, not
`capyio.audio.frames/1` wire interoperability. The upstream protocol discards
CapyIO sequence, epoch and timestamps and is unauthenticated, so it is limited
to an explicitly trusted local/Tailscale lab. Production use requires a new
authenticated transport or a versioned extension plus Android receiver work.

Use the existing locked `prost` 0.14 dependency (Apache-2.0 OR MIT) rather than
hand-writing Protobuf varints. The dependency is already used by
`capyio-protocol`, maintained by the Tokio project and avoids importing the
upstream generated source or schema. `capyio-audio` is an internal workspace
dependency used for format validation.

## Alternatives

- modify and redistribute the upstream C++ server to accept IPC PCM;
- feed a virtual playback endpoint back into the unchanged upstream WASAPI
  loopback server;
- write the tiny Protobuf encoder manually;
- define the final authenticated CapyIO audio transport before the APO/Broker
  path has produced real PCM;
- send PCM through Adapter stdout or node-control messages.

## Consequences

- A deterministic desktop test can now prove PCM-to-Android-protocol behavior
  without installing a driver or requiring a phone.
- The later Broker connects the render staging reader to this bounded sender;
  it does not need to fork or control the upstream Windows server.
- The existing Android app can be used for the next physical tone test without
  an APK change.
- Audible output, Android lifecycle and real APO PCM remain separate physical
  acceptance evidence.

## Reviewed upstream evidence

- Repository: <https://github.com/mkckr0/audio-share>
- Revision: `342751fe675367483170b002ec6054e243966dc0`
- Review archive SHA-256:
  `2cb8b9347ae5f1288b206955f0d8c69717e5d7236e19304510524d2edbdfce64`
- Reviewed paths: `docs/protocol.md`, `protos/client.proto`,
  `server-core/src/network_manager.*`, `server-core/src/audio_manager.*`,
  `server-core/src/win32/audio_manager_impl.*`, and Android service
  `NetClient.kt`, `NetworkIO.kt`, `AudioPlayer.kt`.
- License: Apache-2.0. No upstream source, schema or binary is imported.
