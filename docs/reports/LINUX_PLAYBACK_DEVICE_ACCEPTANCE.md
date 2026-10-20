# Linux playback: prerequisites and real-device acceptance

Date: 2026-10-20. Scope: experimental, finite **playback**, not a virtual microphone.
This guide is a checklist, not permission to provision credentials, install an APK,
change device permissions/network settings or start a listener/capture session.

## What is proven

Exact command implementation `a4ee19afdcc29a3809b5eb1a52e490dff0fb4b32`
passed [hosted CI 39401756387](https://github.com/Arthurshen926/HardwarePool/actions/runs/39401756387):
104 required regression guards, including eight executable command cases with
synthetic paired identities and ephemeral test credentials. Those command cases
exercise actual TLS/catalog, exact local approval, finite offers, Stop, rejection,
EOF and bounded output failure. They intentionally do not send the reciprocal
media offer or start PipeWire. Separate synthetic worker tests cover UDP PCM and
owned subprocess cleanup. None proves Android capture, PipeWire rendering,
audibility, latency, actual credentials or interoperability on physical devices.

## First: passive preflight

From the repository root, build/run the diagnostic without device access:

```text
cargo run --locked -p capyio-node -- linux-playback-preflight
cargo run --locked -p capyio-node -- linux-playback-preflight --target <intended-PipeWire-node-name-or-serial>
```

The JSON report is evidence, not a ready/not-ready deployment verdict. It checks
the fixed `/usr/bin/pw-cat` executable candidate and local runtime metadata without running PipeWire tools,
connecting to a session, enumerating targets, listening, reading credentials or
playing/capturing audio. Version, executable trust, server liveness and target
existence remain unverified even if paths exist. A successful command exit means
the report was produced, **not** that playback will work. Missing prerequisites
are ordinary structured results. Do not install packages or change settings in
response without the relevant approval. No secret input belongs in this command.

## Required deployment inputs and approval gate

Before a real test, the operator must approve the exact devices and private lab
network, authorized credential access/handoff, and any required installation or
permission changes separately. Establish a trusted local launcher with exclusive
inherited FIFO ownership. The repository supplies a framed protocol, not a
production credential manager or interactive consent launcher. Do not paste
credentials into shell arguments, environment variables, logs or this guide.

Required inputs for [ADR 1522](../adr/1522-linux-explicit-playback-command.md):

- Stable non-nil local Node ID; an already issued credential bundle and exactly
  one active paired-peer record matching the intended Source Node.
- The exact Source PortRef from the real authenticated catalog. It must be an
  available mono 48 kHz S16LE audio-frames Source with Interactive QoS. Android
  permission/foreground lifecycle must genuinely make it available; never edit
  a real catalog to bypass a permission gate as synthetic fixtures do.
- Explicit permitted local IPv4 control address/port and media address/nonzero
  port with the same local IP, an intended PipeWire node name/serial, and a
  finite duration from 1 to 300 seconds. Verify actual target identity and
  user-session access separately; passive preflight does not verify either.
- A peer implementation supporting this exact paired control/catalog and
  reciprocal dynamic offer/binding. The older manual Android UDP sender uses a
  fixed lab binding and is **not evidence of compatibility** with this command.

UDP is unauthenticated and unencrypted even though control is paired TLS. Use
only an explicitly authorized isolated/trusted lab. Private addresses and a
certificate-authenticated control peer do not authenticate media packets.

## Exact opt-in workflow (only after the separate approvals)

1. The trusted launcher starts the verified built node executable with
   `linux-playback --trusted-lab` (not an unverified PATH lookup), with
   exclusive FIFO stdin/stdout. Do not use ordinary terminal stdin or redirect
   secret files. Every message is a 4-byte big-endian byte length plus payload,
   not newline JSON. See ADR 1522 for the complete strict schemas and limits.
2. Within the shared five-second bootstrap budget, supply exactly the nonsecret
   config JSON, existing paired-directory JSON, then encoded credential bundle.
   No credentials are created or persisted by the command. Keep both pipes open
   and continuously drain events; blocked output fails within its operation bound.
3. The intended peer connects during the finite accept window and completes
   pinned mutual TLS/fresh binding and real catalog exchange. Reject unexpected
   identities or unavailable/incompatible Sources; do not force readiness.
4. The launcher presents `approval_required`, including the exact Source, Sink,
   target, duration, fresh request ID and Route ID. Obtain explicit local consent
   for this exact challenge. Only then send version 1 `approve` with those exact
   `request_id` and `route_id` fields within the 30-second consent window.
   `status` and `stop` are separate commands, never implicit approval.
5. `awaiting_peer_offer` is not playback readiness. The peer must negotiate the
   reciprocal authenticated offer; capture must retain its visible device consent
   and foreground lifecycle. Confirm rendered sound independently, not from
   packet counts or bytes written to a child pipe.
6. Request local `stop` and observe bounded owned-resource cleanup. Restart with
   a fresh invocation/challenge for another run; there is no automatic renewal,
   reconnect, remembered consent or default background listener. On terminal
   FIFO failure, observe process exit/static stderr: a final JSON event is only
   best effort while the output channel remains usable.

## Minimum real-device acceptance record

Record nonsecret build SHAs, OS/PipeWire and Android app versions, hardware/target
identity, approved network topology and consent timestamps. Never record key
material. For each case retain observable evidence and cleanup outcome:

- Correct paired identity/catalog and exact approval precede any capture/render;
  wrong identity, replayed approval and permission-denied Source fail closed.
- Actual Android mono capture reaches the explicitly selected PipeWire target;
  verify sound and measure duration, loss and latency rather than infer them.
- Local Stop, peer Stop/disconnect, launcher EOF and finite lease expiry stop
  rendering/capture appropriately; owned processes and listeners do not survive.
- Missing/exited PipeWire target or child failure reports failure, not Active;
  restart requires fresh approval and does not reuse an old binding.
- Document any unavailable measurement, cleanup failure or untested case. Do not
  claim acceptance if only the synthetic hosted suite or passive preflight ran.

## Remaining Linux/Windows parity gaps

This Linux path is headless and lab-only: no product pairing/provisioning UI,
trusted launcher implementation, secure UDP media, system virtual microphone,
virtual camera/input/USB projection, packaged service installation or verified
Android/PipeWire device acceptance. Windows has separate platform adapters,
projection/driver and lab workflows; their evidence does not transfer to Linux,
and this guide does not declare all Windows paths production-ready. Existing
Windows/Android behavior is unchanged. Closing Linux playback hardware acceptance
still does not close virtual-device or wider platform parity gates.
