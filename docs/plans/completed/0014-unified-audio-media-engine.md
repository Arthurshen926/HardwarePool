# CAPY-AUDIO-CORE-001 — Unified audio media contracts

Status: complete

Owner: Codex and project owner

Created: 2026-08-28

Completed: 2026-08-28

Depends on: completed `CAPY-AUDIO-001B` functional Gate

## Objective

Extend the existing transport-neutral audio primitives into one bounded,
direction-neutral contract shared by remote microphone and remote speaker
Routes, without changing the physically proven Speaker wire, render ring or
Android playback format.

## Completed scope

1. ADR 0035 records the shared-engine and independent-Route decision.
2. `capyio-audio` defines selected stream specifications, PCM/Opus identity,
   three initial QoS presets, processing requests, bounded deterministic
   negotiation and common metrics.
3. Concrete codecs, sockets, platform APIs and reconnect logic remain outside
   the crate.
4. Audio Share maps the common Speaker specification and observable metrics
   while retaining its pinned private wire bytes.
5. Unit/integration tests cover presets, invalid combinations, negotiation
   order/bounds and Speaker wire regression.
6. Normative audio, architecture, data-plane, roadmap, backlog, testing and
   traceability documents are updated.

## Retained non-scope

- MicYou source or execution;
- codec or network dependencies;
- production transport selection;
- Windows driver/APO/ring or Android APK changes;
- 96 kHz, multichannel or physical voice-processing claims.

## Evidence

All acceptance criteria are satisfied. Automated and authorized physical
evidence, exact counters and unresolved risks are recorded in
`docs/CAPY_AUDIO_CORE_001_REPORT.md`.

## Follow-up Gate

`CAPY-MIC-000` pins and audits MicYou before any source import. Its GPL-3.0
license and exact integration boundary must be recorded before implementation.

