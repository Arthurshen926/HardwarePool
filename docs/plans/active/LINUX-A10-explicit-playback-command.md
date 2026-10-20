# LINUX-A10: Explicit one-shot Linux playback command

Date: 2026-10-20
Status: implemented; independent review and exact-head hosted validation pending.
Base: 15213546a436793f3fd32a82d21f6997c030bbb9
ADR and complete local schema: [1522](../../adr/1522-linux-explicit-playback-command.md)

## Acceptance

- Separate `linux-playback --trusted-lab`; defaults remain closed.
- Bounded one-shot FIFO handoff of existing credentials and paired record;
  fixed errors, unnecessary transient secret copies cleared, no persistence.
- Actual pinned TLS/catalog before fresh exact-Route local approval; finite
  authorization/lease created only by that approval, never a peer assertion.
- Finite accept/handshake/phase deadlines, local Stop/status on supervisor thread,
  failure/EOF/expiry cleanup, no detached blocking stdin reader.
- Pure tests plus compiled hosted subprocess/TLS cases, strict lint/format and
  repository validation. No actual credentials, local IPC or hardware runs.

## Remaining gate

Publish reviewed source and pass exact-head hosted guards. Deployment requires
separate approved credential access/handoff and exclusive trusted launcher,
explicit device/network setup authorization where applicable, and real paired
Android/PipeWire acceptance. UDP remains insecure lab transport. Do not describe
stdin as secure merely because it is inherited or treat this code as permission
to deploy a listener or configure credentials on a user system.
