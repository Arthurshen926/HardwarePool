# LINUX-A06 — Android-compatible Linux PCM playback

Status: independently reviewed; hosted synthetic IPC accepted on `4b5db1a`; device acceptance pending
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

## Hosted synthetic acceptance (2026-10-20)

Exact implementation `4b5db1a342994365dab9ccff93b670a72fcba49d` passed
[run 39353185940, job 118401894752](https://github.com/Arthurshen926/HardwarePool/actions/runs/39353185940/job/118401894752)
on GitHub-hosted Ubuntu 24.04.5 with Rust 1.97.1. Formatting, strict Clippy and
mock binary builds passed. Full logs verify 79 executed tests: 13 Linux audio,
40 prior Linux host/mock/node tests and 26 existing native LAN tests. There were
zero failures, ignored tests or filtered tests. All 34 required successful test
outcomes were checked, including synthetic Android-wire UDP-to-owned-child exact
PCM bytes, full-stdin Stop/reap, EOF/repeated Stop and early child exit.

This closes the synthetic hosted IPC gate only. No actual `pw-cat`, PipeWire
session, Android device, physical audio capture or audible playback was tested.
Virtual microphone, Runtime dynamic Route/grant/lease ownership and authenticated
media remain separate gates. The restricted cloud environment's local socket
limits are unchanged. A later documentation-only evidence commit is not the
tested implementation SHA.
