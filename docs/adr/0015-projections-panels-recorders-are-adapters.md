# ADR 0015: Projections, Panels and Recorders are Adapter capabilities

- Status: Accepted
- Date: 2026-08-23

## Context

Making every OS or ecosystem mechanism a top-level Core object would grow the
domain without adding shared invariants.

## Decision

System projections, Panels, Recorders, ROS topics, USB/IP devices and virtual
devices are Adapter-owned Capabilities/Ports. Core remains mechanism-agnostic.

## Alternatives

Separate Projection/Panel/Recorder/Topic lifecycles in Core; hide them entirely
inside UI.

## Consequences

UI can group mechanisms while Route validation stays uniform. Mechanism-specific
metadata lives in Adapter/Profile descriptors.

