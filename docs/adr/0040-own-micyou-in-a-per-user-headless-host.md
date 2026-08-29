# ADR 0040: Own MicYou in a per-user headless Windows host

Status: accepted

Amends: ADR 0039's temporary Tauri-host lifecycle ownership.

## Context

The microphone functional Gate proved Android capture, the MicYou private media
path, the paired Windows endpoints and a Runtime-owned Route. The desktop host
still directly owns the MicYou child, however, so closing CapyIO Desktop stops
an active microphone Route. Moving that child into the LocalSystem
`CapyIOBroker` service would solve window lifetime but would also make a
privileged machine-wide principal read user-local configuration, select a
per-user audio endpoint and launch a separately supplied GPL process.

The capture ring has a different boundary: AudioDG needs one cross-session
mapping, so `CapyIOBroker` already creates and owns it. Ring ownership does not
require the same principal to own the MicYou network/decoder process.

## Decision

Keep `Global\\CapyIO.CaptureRing.v1` in the privileged `CapyIOBroker` service
and add an ordinary-user `capyio-microphone-host` process for MicYou lifecycle.
The host reads only ADR 0039's fixed user-local trusted configuration, creates
the existing MicYou supervisor and keeps running independently of the desktop
window. A future installer may start exactly one instance at user logon; this
slice does not install or register autostart.

Expose one local byte-mode named pipe,
`\\.\pipe\CapyIO.Microphone.Control.v1`. It reuses the same bounded framing and
I/O implementation as the Speaker service but has an independent schema-v1
contract and lifecycle. Requests are limited to `status`, `start` and `stop`;
closed JSON frames are at most 4 KiB. Responses contain only typed lifecycle
state, generation, validated bind address, peer-presence state and a stable
problem code. Executable paths, endpoint IDs/names and arbitrary arguments are
never accepted or returned.

The pipe rejects remote clients. Its protected DACL grants the object owner,
LocalSystem and Administrators full access rather than granting every
interactive user access to a microphone Route. The host independently bounds
the initial phone wait and stops/reaps MicYou after active phone loss, even when
the desktop is absent.

CapyIO Desktop probes the per-user host with a short bounded availability
window and uses it when present. The existing direct supervisor remains a
development fallback. When a
headless Route is already running, the desktop adopts its state; closing the
desktop does not send `stop`. Microphone and Speaker retain separate pipes,
Routes, authorization and failure state while sharing the direction-neutral
audio media model and the local pipe transport implementation.

## Consequences

- Normal microphone lifecycle no longer needs an elevated UI or a LocalSystem
  third-party child process.
- The privileged service and per-user host have deliberately different duties:
  global audio-memory ownership versus user-scoped process/config ownership.
- The host pipe is a local principal boundary only. It does not authenticate
  the Android peer or make MicYou's private TCP/UDP transport production-safe.
- Login autostart, single-instance recovery, host binary signing, hardened
  configuration ACLs, crash restart, privacy UI and multi-user acceptance
  remain release work.
