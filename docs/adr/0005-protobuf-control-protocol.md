# ADR 0005: Protobuf for versioned control messages

Status: accepted for bootstrap

## Context

The project requires a compact typed cross-language protocol. JSON is useful for diagnostics but weak as the only long-term wire contract.

## Decision

Use Protobuf v1 for control-plane messages. Keep Core types separate and implement explicit conversion. Use compact non-JSON frames for real-time audio data.

## Consequences

- Field-number compatibility rules become mandatory.
- Generated code is build output rather than hand-edited source.
- Debug/UI JSON remains separate from the public wire protocol.
- The protocol is not coupled to WebSocket, QUIC, AOO or another transport.
