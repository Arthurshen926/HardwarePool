# Glossary

- **Adapter** — platform or transport implementation behind a stable interface.
- **Binding** — per-session lifecycle of one remote Capability.
- **Broker** — Windows user-mode process responsible for network, protocol, codec, buffering and driver IPC.
- **Bundle** — logical reference to multiple capabilities without merging their state or permission.
- **Capability** — independently described, authorized and used hardware/software resource.
- **Clock domain** — monotonic sample timing source associated with an audio device/stream.
- **Core** — OS-independent domain model and lifecycle rules.
- **Data plane** — high-frequency media or sensor payload path.
- **Epoch** — a continuous stream interval with one format, sample origin and clock domain.
- **Lease** — time-bounded authorization to use a capability.
- **Node** — HardwarePool runtime instance on a device/process.
- **Profile** — versioned semantics for a capability class.
- **Projection** — local representation of a remote capability.
- **Runtime** — orchestration layer that owns peers, sessions, commands, events and snapshots.
- **System capture endpoint** — OS recording device such as a virtual microphone.
- **System render endpoint** — OS playback device such as a virtual speaker.
- **Transport binding** — concrete network/IPC mechanism carrying protocol semantics.
