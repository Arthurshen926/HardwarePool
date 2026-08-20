# ADR 0004: Microphone and speaker are independent capabilities

Status: accepted

## Context

A phone's microphone and speaker are physically related but have different data directions, permissions, failure modes, QoS and user intent.

## Decision

Model them as independent `audio.capture/1` and `audio.render/1` Capability instances. A duplex bundle references both and advertises relationship metadata.

## Consequences

- Each can be authorized and stopped independently.
- Full-duplex partial failure is representable.
- AEC relationship can be modeled without merging permissions.
- UI must expose two controls, even if it offers a convenience “start both” action later.
