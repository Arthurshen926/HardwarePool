# Audio Share external-process Adapter

`CAPY-AUDIO-001A0/001A1` provides the bounded boundary for the pinned,
unmodified Audio Share v0.3.4 CLI. It validates an explicit bind address,
playback endpoint and PCM settings, parses bounded version/endpoint output and
runs read-only probes without a command shell. The supervisor starts the CLI
directly, waits only for its TCP listener, drains output with fixed retention,
types startup/exit/timeout states and stops/reaps the child idempotently.

`CAPY-AUDIO-001B2T` additionally provides a CapyIO-authored, wire-compatible
sender for the pinned Android app's private TCP/UDP contract. It validates a
`capyio-audio` format, bounds peers, queued blocks and block bytes, segments PCM
on sample-frame boundaries and expires heartbeat sessions. Its loopback test
proves format negotiation and segmented PCM delivery without a driver or phone.

This crate contains no Audio Share source or binaries and does not advertise a
StandardPort transport. The compatible TCP/UDP data plane remains an
`AdapterManaged`, unauthenticated trusted-lab contract. The legacy CLI has no
machine-readable peer-status API, so listener readiness is not claimed as
Android playback/receiver health and ordinary logs are not parsed for behavior.
Runtime Route lifecycle and UI projection are later slices in the active plan.

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

The deterministic 48 kHz stereo S16 tone path has been physically submitted to
the Android receiver and heard by the human operator. The Windows-only
`capyio-virtual-speaker` Broker now replaces that tone source with the bounded
render APO shared-memory reader and float32-to-S16 conversion.

For that bounded physical lab, bind an explicit host IPv4 address and configure
the same host and port in the pinned Android Audio Share app:

```text
cargo run -p capyio-audio-share-adapter --bin capyio-audio-share-tone -- 100.64.x.y:65530
```

The command waits at most 60 seconds for TCP plus UDP association, sends a quiet
440 Hz tone for 10 seconds, then exits. Audible output is human-observed evidence
and must be recorded separately from the deterministic transport test. Its
final line reports bounded queue and UDP counters so a successful process exit
is not mistaken for proof that PCM was actually submitted to the socket.

After the approved driver package is installed, start the Broker before opening
an application stream on `CapyIO Speaker`:

```text
cargo run -p capyio-audio-share-adapter --bin capyio-virtual-speaker -- 100.64.x.y:65530 [duration-seconds]
```

The optional duration is limited to 1-300 seconds and prints bounded ring and
transport counters on completion for lab evidence. The current lab process must
run elevated so it can own the cross-session
`Global\\CapyIO.RenderRing.v1` mapping used by Session 0 AudioDG. Its protected
ACL grants only Local Service, Local System and administrators access. A second
Broker is rejected. Ring loss, invalid layout and transport shutdown are typed
failures; ring-empty polling is bounded and happens only in user mode.

For the desktop Runtime-owned path, configure the same explicit bind IP/port
and the repository-built Broker executable in the trusted host environment:

```text
CAPYIO_VIRTUAL_SPEAKER_EXE=C:\path\to\capyio-virtual-speaker.exe
CAPYIO_AUDIO_SHARE_BIND_IP=100.64.x.y
CAPYIO_AUDIO_SHARE_PORT=65530
```

This fixed mode does not use `CAPYIO_AUDIO_SHARE_ENDPOINT` and hides the legacy
endpoint picker. Start/stop from the Quick Action owns and reaps the Broker.
