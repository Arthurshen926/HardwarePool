# ADR 0017: Provide Quick Actions and Workspace

- Status: Accepted
- Date: 2026-08-23

## Context

A technical Route/Port workspace is powerful but too complex as the only entry;
a task-only UI hides the platform's composability and diagnostics.

## Decision

Quick Actions are versioned Route Templates for ordinary workflows. Workspace /
Lab exposes Nodes, Capabilities, Ports, Routes, Adapters and Problems. Initial
Panels are built in and Route editing uses accessible lists/cards.

## Alternatives

Workspace only; Quick Actions only; node-graph editor first.

## Consequences

DTOs must serve both modes and internal terms require plain-language mapping.

