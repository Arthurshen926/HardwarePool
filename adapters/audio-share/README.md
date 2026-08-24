# Audio Share external-process Adapter

`CAPY-AUDIO-001A0/001A1` provides the bounded boundary for the pinned,
unmodified Audio Share v0.3.4 CLI. It validates an explicit bind address,
playback endpoint and PCM settings, parses bounded version/endpoint output and
runs read-only probes without a command shell. The supervisor starts the CLI
directly, waits only for its TCP listener, drains output with fixed retention,
types startup/exit/timeout states and stops/reaps the child idempotently.

This crate does not contain Audio Share source or binaries, send PCM, connect to
Android or advertise a StandardPort. The upstream TCP/UDP data plane remains an
`AdapterManaged` private contract. The CLI has no machine-readable peer-status
API, so listener readiness is not claimed as Android playback/receiver health
and ordinary logs are not parsed for behavior. Runtime Route lifecycle and UI
projection are later slices in the active plan.

On Windows, the Adapter can separately query the documented process-owned TCP
table and report `ReceiverTcpPresence`. It filters by the supervised PID,
explicit local port and `ESTABLISHED` state without returning peer addresses.
This signal remains transport presence, not protocol or audible-playback proof.

The ignored real probe requires a user-supplied executable path:

```text
CAPYIO_AUDIO_SHARE_EXE=C:\path\to\as-cmd.exe cargo test \
  -p capyio-audio-share-adapter -- --ignored
```

The ignored real supervisor additionally requires an explicit endpoint and
unused port; it starts Windows system-loopback capture briefly and always stops
and reaps the process in the test.
