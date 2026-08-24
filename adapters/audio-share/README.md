# Audio Share external-process Adapter

`CAPY-AUDIO-001A0` provides the hardware-free boundary for the pinned,
unmodified Audio Share v0.3.4 CLI. It validates an explicit bind address,
playback endpoint and PCM settings, parses bounded version/endpoint output and
runs read-only probes without a command shell.

This crate does not contain Audio Share source or binaries, start a server, send
PCM, connect to Android or advertise a StandardPort. The upstream TCP/UDP data
plane remains an `AdapterManaged` private contract. Process supervision,
Runtime Route lifecycle and UI projection are later slices in the active plan.

The ignored real probe requires a user-supplied executable path:

```text
CAPYIO_AUDIO_SHARE_EXE=C:\path\to\as-cmd.exe cargo test \
  -p capyio-audio-share-adapter -- --ignored
```
