# CAPY-MIC-001 — Windows `CapyIO Microphone`

Status: active

Owner: Codex and project owner

Created: 2026-08-28

Depends on: `CAPY-MIC-000A`, completed Speaker Gate 7B

## Objective

Expose one independent Windows capture endpoint named `CapyIO Microphone` and
replace its SysVAD test-tone input with bounded 48 kHz mono PCM supplied by the
user-mode CapyIO Broker.

## In scope

1. Restore only the minimal SysVAD microphone endpoint beside the existing
   Speaker endpoint and prove the package builds.
2. Define a versioned, fixed-capacity capture ring whose absence produces
   silence without blocking AudioDG.
3. Add a capture APO that consumes committed PCM blocks and overwrites the
   capture output with silence on underrun/stale generation.
4. Add a user-mode producer that accepts decoded microphone-baseline PCM.
5. Connect the MicYou Adapter boundary, then validate ordinary-app recording.

## Out of scope

- network, codec or reconnect logic in the driver/APO;
- copying or linking MicYou GPL source;
- automatic driver/APK installation;
- AEC, denoise and AGC performance claims before the raw path is proven;
- public signing, packaging or redistribution.

## Architecture constraints

- the capture Route is independent of Speaker permissions and lifecycle;
- the real-time APO callback performs bounded copies/zero-fill and atomics only;
- missing Broker, phone loss, underflow and invalid generation degrade to
  silence, never a wait or audio-service failure;
- the first PCM epoch is 48 kHz, mono, signed 16-bit decoded audio;
- MicYou's private transport remains AdapterManaged outside the audio service.

## Acceptance criteria

1. WDK x64 Release build and INF validation pass.
2. The package exposes exactly one CapyIO render and one CapyIO capture endpoint.
3. With no Broker the microphone records silence, not the SysVAD tone.
4. A deterministic injected tone is recorded by an ordinary WASAPI application.
5. Phone audio is recorded by an ordinary application and disconnect returns
   to silence within the bounded underflow window.
6. Install/restart/uninstall and rollback evidence is retained under ADR 0029.

## Safety

Source/build work is unprivileged. Every install, upgrade, uninstall or driver
tool invocation still requires explicit approval for the exact package.

## Current progress

- enumeration-only capture miniport and INF declarations added;
- x64 Release WDK compile/link/API validation passed with MSVC `/W4 /WX`;
- independent x64 InfVerif `/u` and `/w` passed; the known WDK embedded
  `x86\\InfVerif.dll` loader defect remains visible as three managed-task errors;
- capture shared-memory ABI, APO ingestion, Broker producer and physical tests
  remain pending.
