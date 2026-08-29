# MicYou Adapter

This crate is CapyIO's bounded process boundary for the pinned MicYou v2.0.1
CLI. MicYou remains a separate GPL-3.0-only program and owns its private
TCP/UDP audio transport, decoding, jitter buffering and DSP. No MicYou source,
schema, library or binary is copied, linked or distributed here.

The current slice:

- verifies the exact `micyou-cli 2.0.1` version and the CapyIO
  `device-stable-id-v1` capability;
- preserves a bounded output-device inventory, including duplicate display
  names, and persists a stable endpoint ID plus expected name rather than an
  enumeration index;
- freshly resolves the current one-based launch index for that ID before every
  spawn and passes an ID/index/name tuple that the patched CLI validates;
- starts Wi-Fi mode with an explicit IPv4 bind address and port;
- reports only a bounded process-owned TCP peer count through the shared
  Windows platform helper; this is connection evidence, not PCM evidence;
- drains child output with bounded retention and owns stop/reap behavior;
- stops and reaps the receiver when an active phone session is lost, so an
  explicit retry starts a new process and Route epoch;
- maps the Adapter-managed route to CapyIO's shared voice audio policy.

The adapter targets the CapyIO-owned paired Windows microphone endpoints. The
pinned upstream CLI must be locally rebuilt with the reviewed fail-closed
stable-endpoint patch; an unmodified v2.0.1 binary is rejected. Do not run
`micyou-cli mics --install` from this Adapter: that performs a separate driver
installation and needs its own approval and provenance review.

The controlled-lab CLI also carries the reviewed Windows mode-lock correction
recorded in `third_party/THIRD_PARTY.yml`: a retained lock is live only when
both its PID and process creation time match. This avoids false “already
running” failures when Windows reuses the PID of a receiver that CapyIO
previously terminated.

## Trusted desktop configuration

CapyIO Desktop accepts a complete `CAPYIO_MICYOU_*` environment override for
development. When no override field is present it automatically loads the
schema-v1 host file at `%LOCALAPPDATA%\CapyIO\host\micyou-v1.json`. The file is
host-only; its executable path and endpoint ID never enter the WebView DTO.

After inspecting the patched CLI's `devices` inventory, provision the selected
stable ingress ID once with the repository host tool:

```text
cargo run -p capyio-micyou-host-config --bin capyio-micyou-config -- provision --executable <path-to-patched-micyou-cli> --bind-ip <phone-reachable-windows-ip> --endpoint-id <exact-stable-ingress-id>
```

The tool derives the expected name from the probed inventory and never stores
the temporary enumeration index. It creates a new configuration and refuses to
overwrite an existing file. Validate the current executable and endpoint later
with:

```text
cargo run -p capyio-micyou-host-config --bin capyio-micyou-config -- validate
```

Neither command downloads MicYou, installs a driver/APK or changes Android
permissions.
