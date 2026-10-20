# LINUX-A06 — Android-compatible Linux PCM playback

Status: implemented locally; independent review and hosted IPC acceptance pending
Created: 2026-10-20
Requirements: `FR-SCEN-001`, `FR-PLAT-004`, `NFR-STAB-002`, `NFR-SEC-005`

## Scope and acceptance

Implement one opt-in, finite Linux playback path using the existing Android
native microphone lab binding and UDP codec, not another mock batch or wire.
An isolated PipeWire `pw-cat` process consumes exact mono 48 kHz S16LE PCM via
a bounded nonblocking local stream. Default Linux host behavior stays unchanged.
No modules, drivers, services, credentials, permissions or APK changes. No local
socket execution or physical capture/playback during implementation.

Required checks: explicit lab policy, private/loopback endpoints, finite lifetime,
exact binding/format, stale sequence rejection, bounded partial-write/backpressure,
owned-child failure/cleanup, and hosted synthetic UDP-to-PCM IPC integration.
Do not claim peer authentication, Runtime Route integration, system microphone,
actual PipeWire playback or Android-device parity from these tests.

## Next gate, not part of this implementation

Add a separately reviewed Linux virtual microphone projection and authenticated
control/Route/lease ownership. Secure audio transport also needs coordinated
Android changes; today's paired TLS control does not encrypt the UDP media.
A later authorized Linux-session/Android-device test must prove actual playback,
then application-visible microphone delivery, independent Stop and reconnect.

## Current evidence

Nine socket-free tests passed locally using Rust 1.97.1. The whole new crate
compiled and its tests compiled; local socket execution remains excluded.
The external `pw-cat` executable is absent in this environment. No real session,
Android device, physical capture or playback was used. See ADR 1518 for exact
security, dependency and evidence boundaries.
