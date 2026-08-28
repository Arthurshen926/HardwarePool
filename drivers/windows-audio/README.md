# Windows Virtual Audio Driver Slot

This directory contains the Gate 7B Windows system-projection spike that exposes:

- `CapyIO Speaker` as a Windows render endpoint;
- an enumeration-only `CapyIO Microphone` capture endpoint baseline. It still
  emits the SysVAD test tone until the bounded capture-APO ingress is complete.

The speaker implementation is a minimized MS-PL SysVAD derivative with
CapyIO-owned device, service, APO and extension identifiers. Its source and
local modifications are recorded in `third_party/THIRD_PARTY.yml`. Deployment
defaults to an isolated Windows target; ADR 0029 defines the single controlled
local-lab exception.

## Required process boundary

```text
Windows Audio Engine
        |
CapyIOAudio.sys
        |
CapyIO render APO (real-time, user mode)
        |
bounded shared-memory/SPSC staging ring
        |
CapyIO Broker (user mode)
        |
Core / protocol / transport / codec / network
```

The driver is deliberately thin. It must not contain sockets, discovery,
pairing, TLS, Protobuf, JSON, Opus, AOO, WebRTC, reconnect policy, UI, or user
configuration. The first speaker slice uses a render APO because SysVAD's
WASAPI loopback is a synthetic tone, not the application's real render PCM.
The APO may only copy into preallocated bounded staging and update counters;
all transport behavior remains in the Broker. `IPC_CONTRACT.md` defines this
user-mode bridge and retains a custom driver IPC as a deferred fallback.

## Intended milestones

1. Build an unchanged SysVAD sample locally; install it only in an approved
   isolated target or the ADR 0029 local lab after recovery preflight.
2. Prove the render endpoint can enumerate and survive restart/uninstall.
3. Prove an endpoint-associated render APO receives real PCM and can copy it
   into bounded staging without blocking the real-time callback.
4. Connect the Broker side of that staging ring to the existing Audio Share transport and prove the
   physical/RDP endpoint remains silent.
5. Exercise ring-full, Broker loss, format-epoch and restart behavior; activate
   a small Broker/driver IPC only if the APO evidence fails.
6. Run Driver Verifier and applicable HLK tests.
7. Add signing and installer workflows only after functional stability.

Read `IPC_CONTRACT.md`, this directory's `AGENTS.md`, `docs/ARCHITECTURE.md`, and `docs/SECURITY_MODEL.md` before adding code.
