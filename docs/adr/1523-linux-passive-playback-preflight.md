# ADR 1523: Passive Linux playback preflight

Status: implemented; independent review and exact-head hosted validation pending.
Date: 2026-10-20

## Decision

Expose `capyio-node linux-playback-preflight [--target <name-or-serial>]` as a
separate read-only JSON diagnostic. It does not invoke the playback launcher or
consume its bootstrap protocol. Default commands and the explicit lab opt-in
remain unchanged. Linux metadata inspection lives in `capyio-linux-host`, not
Core/Runtime or the thin CLI. No dependencies or network protocol changes.

Inspect only bounded prerequisite metadata and validate optional target syntax
with the existing adapter policy. Never run discovered tools, read credentials,
connect to PipeWire, enumerate devices, capture/play audio, create a listener,
install software or alter permissions/settings. PipeWire version and target
existence therefore remain explicitly unqueried. A metadata candidate is neither
trusted executable content nor proof of a live server. Readiness always remains
`not_verified`; exit success means diagnostic JSON was produced, not deployment
approval. Missing runtime/tools are structured findings, not a crash.

Diagnostics do not echo environment values or filesystem paths. Metadata probes
are finite in count but filesystem syscalls have no guaranteed wall-clock bound
on a stalled remote filesystem; no detached timeout worker disguises that limit.
The report contains static required-input and limitation descriptions. The target
input is only syntax checked, not authorization and not credential material.

## Evidence and next gate

Pure tests cover missing/incomplete prerequisites, invalid targets, bounded
inspection and redaction without opening sockets or running installed tools.
CLI parsing and a hosted executable JSON regression cover separate command
selection and missing session diagnostics. No actual PipeWire/Android acceptance
is implied. See the [real-device acceptance guide](../reports/LINUX_PLAYBACK_DEVICE_ACCEPTANCE.md)
for the approved-deployment prerequisites, exact one-shot workflow, minimum
physical evidence and remaining Linux/Windows parity boundaries.
