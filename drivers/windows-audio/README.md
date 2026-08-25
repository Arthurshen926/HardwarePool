# Windows Virtual Audio Driver Slot

This directory is the future home of the Windows system-projection component that exposes:

- `CapyIO Speaker` as a Windows render endpoint;
- `CapyIO Microphone` as a Windows capture endpoint.

No kernel driver source is included in the bootstrap archive. The first implementation must be developed and tested from an isolated Windows target, beginning with an unmodified Microsoft SysVAD build and an explicit licensing review.

## Required process boundary

```text
Windows Audio Engine
        |
CapyIOAudio.sys
        |
standard WaveRT render + WASAPI loopback
        |
CapyIO Broker (user mode)
        |
Core / protocol / transport / codec / network
```

The driver is deliberately thin. It must not contain sockets, discovery,
pairing, TLS, Protobuf, JSON, Opus, AOO, WebRTC, reconnect policy, UI, or user
configuration. The first speaker slice prefers standard user-mode WASAPI
loopback over a custom driver PCM ring. `IPC_CONTRACT.md` remains a deferred
fallback for capabilities or status that standard Windows APIs cannot expose.

## Intended milestones

1. Build and install an unchanged SysVAD sample in a test VM.
2. Prove one render and one capture endpoint can enumerate and survive restart/uninstall.
3. Prove user-mode loopback captures only the dedicated render endpoint.
4. Connect that endpoint to the existing Audio Share transport and prove the
   physical/RDP endpoint remains silent.
5. Define and fuzz a small Broker/driver IPC only if standard APIs are
   insufficient.
6. Run Driver Verifier and applicable HLK tests.
7. Add signing and installer workflows only after functional stability.

Read `IPC_CONTRACT.md`, this directory's `AGENTS.md`, `docs/ARCHITECTURE.md`, and `docs/SECURITY_MODEL.md` before adding code.
