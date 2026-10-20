# CapyIO Platform Support Matrix

This table separates architecture intent from evidence.

| Platform | Foundation Core | Current host/UI evidence | Real Adapters | System Projection evidence |
|---|---|---|---|---|
| Windows | CI target/intended | local Rust workspace and Tauri build | service-owned Audio Share-compatible Broker physically exercised | installed `CapyIO Speaker` render-APO bridge; playback and endpoint volume physically proven on the ADR 0029 lab |
| Android | shared model intended | pinned Audio Share receiver physical lab | Audio Share-compatible speaker sink | app-level speaker playback physically proven; no system Projection |
| Linux | hosted Core/UI CI history | bounded explicit one-peer launcher; paired TLS/catalog/consent/leases and Android coordinator software tests | mono Android-compatible PCM → PipeWire playback and owned virtual microphone; trusted-lab UDP | native PipeWire Audio/Source directly read by an independent recorder in a private hosted graph; physical devices untested |
| macOS | hosted Core CI history | no current local platform run | none | none |
| iOS/iPadOS | future app-level host | none | none | no global virtual-device promise |
| Embedded | future compact Node | none | none | API/protocol only initially |

“Intended” is not “tested”. Add support only with linked CI or hardware evidence.
The deterministic Windows/Android fixtures are cross-platform tests, not
physical platform support.

## Linux software milestone — 2026-10-20

Implementation `510aea94` passed all three existing hosted gates in
[run 39453132776](https://github.com/Arthurshen926/HardwarePool/actions/runs/39453132776).
The private graph covers mono 48 kHz S16LE Android-compatible UDP → playback,
and the owned virtual Audio/Source with independent recorder readback, no-consumer
Stop, owned-child exit and server disconnect. It is not a renamed sink monitor.

The graph uses the Linux-led proposal sequence. Advertised-capability Android
selection and optional incoming-Draft admission are separately covered by IPC and
JNI/Android contract tests; combined incoming-Draft real-graph audio is not proven.
No physical Android capture or user-session PipeWire test has been performed for
this milestone. Native PipeWire evidence does not establish PulseAudio/ALSA
compatibility. Media remains unencrypted/unauthenticated trusted-lab UDP.

This is an explicit single-peer, finite-lifetime launcher using existing paired
records, exact approval and leases, not a complete Linux pairing UI, discovery,
provisioning or desktop product. Default playback remains unchanged. See the
[Linux boundary](../platform/linux/README.md) and
[ADR 1530](adr/1530-linux-owned-virtual-microphone.md) for details and next scope.
