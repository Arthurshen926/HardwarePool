# ADR 1518: Opt-in Android-compatible Linux PCM playback

Status: implemented; local socket-free and hosted synthetic IPC checks passed; device acceptance pending.
Date: 2026-10-20
Plan: [LINUX-A06](../plans/active/LINUX-A06-audio-playback.md)

## Decision

Add the separate `capyio-linux-audio-playback` executable and platform-owned
`capyio-linux-audio` crate. The default `linux-host`, its mock fixtures, Core,
Android application and existing wire contracts are unchanged. This first real
Linux endpoint boundary is **playback**, not a virtual system microphone and
not feature parity with the Android/Windows application.

The executable requires five explicit arguments: `--trusted-lab`, literal local
IPv4:port, literal expected peer IPv4:port, duration (1–300 seconds), and a specific
PipeWire target name/serial. Only private, loopback and shared-overlay IPv4 ranges
are accepted; wildcard/public/multicast/broadcast-like addresses and port zero
are rejected. These restrictions and the flag are scope acknowledgement, not
peer authentication or a target's permission grant. No default listener exists.

Reuse `microphone_lab_binding` and `NativeLanUdpEndpoint` unchanged: the a200…
Session/Route/Stream IDs, epoch 1, exact mono 48 kHz S16LE/10 ms (960-byte) packets,
fixed 1,200-byte datagram bound, two in-flight reassemblies and expected UDP peer.
The existing Android `NativeMicrophoneConfigLoader.fromPeer` manual lab path
uses those IDs. The integrated `AndroidAudioUseModule` dynamic Route path does
not: this executable does not connect its authenticated offers, local grants or
leases, and cannot fall back from a dynamic Route to these static identities.

The pure worker pump validates exact packet binding/format; accepts sequence
gaps but rejects duplicate/decreasing/reset sequence, backward sample positions
and non-increasing timestamps. Its one 960-byte staging buffer preserves partial
write offsets and drops newest complete packets when occupied. Even incremental
progress cannot renew its 250 ms total pending-packet budget. Counter updates
saturate. Lifetime and monotonic-clock checks run before every staging/write step.
No automatic restart or epoch reset occurs. This is local stale-packet filtering,
not cryptographic replay protection, cross-launch freshness or jitter recovery.

A direct `/usr/bin/pw-cat` child receives fixed raw playback arguments and one
validated `--target=` argument, never a shell or arbitrary executable path. Fixed
per-stream `node.dont-fallback`, `node.dont-reconnect` and `node.dont-move`
[properties](https://docs.pipewire.org/page_man_pipewire-props_7.html) prohibit
session-manager fallback/movement away from that target; they do not alter global
PipeWire settings. The
stdin boundary uses a Unix stream pair with a nonblocking parent writer. Child
stdout/stderr are discarded to avoid unbounded retention and accidental payload
logging; an early exit is an explicit endpoint error. `--format=s16` is used only
on little-endian Linux. The PipeWire graph can resample to its device rate; this
is not proof of negotiated device format or bit-perfect output.

The kernel's socket buffer is OS-bounded but not part of the 960-byte user-space
staging limit. This implementation makes no universal end-to-end latency bound.
The single worker owns network and pipe I/O; no OS realtime callback does either.
Inherited stdin is lifecycle only: EOF or any byte requests Stop. One reader
thread owns no media/child handles; it can stay blocked until process exit but
cannot delay cleanup. Receive waits at most 10 ms per call, with deadline/Stop
rechecks before forwarding. Pending PCM is discarded on Stop/expiry/error.

Cleanup closes PCM stdin, polls for EOF exit for 100 ms, then kills only the
owned child and polls for one second. Cleanup failures are reported; no unbounded
`wait` is used. Repeated Stop is safe. Forced termination of the parent is not
covered by this graceful cleanup contract and may leave the child until its
stdin closes or it otherwise exits. No service, driver, module, device permission,
firewall, credential, system graph default or Android manifest is changed.

## Security and evidence limits

Native UDP has no authentication, confidentiality, integrity or cryptographic
replay protection. A private IP or overlay-looking address does not prove an
encrypted overlay is active. Static bindings can be spoofed/replayed across
launches. Never use this on an untrusted network or call it paired/production-safe.
A separate secure audio transport requires coordinated Android changes; existing
paired control TLS alone does not secure these media datagrams.

Diagnostics distinguish complete UDP packets from PCM bytes/packets copied to
the child pipe. They never mark a Runtime Route Active, report a system
microphone, or claim audible/rendered playback. No media is logged or recorded
by the application. CI's file bytes are synthetic test data only.

## External dependency note

PipeWire is an independently installed OS/runtime dependency, not vendored or
linked. No new Cargo third-party dependency was added. Upstream is
[PipeWire](https://pipewire.org/); its
[license declaration](https://github.com/PipeWire/pipewire/blob/master/LICENSE)
lists MIT with specific plugin exceptions. No upstream source or binary is
imported, pinned, modified or redistributed by this change. A future package
must record its exact distribution version and applicable plugin licenses.

The [official pw-cat interface](https://docs.pipewire.org/page_man_pw-cat_1.html)
defines stdin playback and explicit format/rate/channel/target options. PipeWire
is actively maintained; distro versions and these options still require real
session acceptance. Native PipeWire FFI was deferred to avoid new unsafe ABI and
callback ownership; ALSA-only playback would not cover the common session graph.
`pw-cat`'s isolated process is the smallest existing endpoint integration, not
an assertion that its native buffering is controlled by this crate.

## Validation

Local: nine socket-free tests, compile and strict Clippy; no local socket or
physical audio execution. Hosted required tests cover exact UDP-to-owned-child
synthetic PCM bytes, EOF/repeated Stop, early exit and actual full-stdin cleanup.
The inherited-owner stdin reader has a socket-free EOF/byte/error test; the
child EOF test proves a distinct process boundary. A whole-CLI real PipeWire
cancellation test remains part of device acceptance. The existing shared
Rust/Java golden UDP test remains required. A missing or
failed hosted test must not be reported as acceptance. No `pw-cat`/PipeWire
session or Android physical-device result exists for this slice.

Next gates: virtual microphone projection; Runtime-owned dynamic negotiated
Route/grant/lease lifecycle and reconnect; secure Android-compatible audio
transport; separately authorized device playback and application-capture tests.

## Hosted synthetic acceptance (2026-10-20)

Exact implementation `4b5db1a342994365dab9ccff93b670a72fcba49d` passed
[run 39353185940, job 118401894752](https://github.com/Arthurshen926/HardwarePool/actions/runs/39353185940/job/118401894752)
on GitHub-hosted Ubuntu 24.04.5 with Rust 1.97.1. Formatting, strict Clippy and
mock binary builds passed. Full logs verify 79 executed tests: 13 Linux audio,
40 prior Linux host/mock/node tests and 26 existing native LAN tests. There were
zero failures, ignored tests or filtered tests. All 34 required successful test
outcomes were checked, including synthetic Android-wire UDP-to-owned-child exact
PCM bytes, full-stdin Stop/reap, EOF/repeated Stop and early child exit.

This closes the synthetic hosted IPC gate only. No actual `pw-cat`, PipeWire
session, Android device, physical audio capture or audible playback was tested.
Virtual microphone, Runtime dynamic Route/grant/lease ownership and authenticated
media remain separate gates. The restricted cloud environment's local socket
limits are unchanged. A later documentation-only evidence commit is not the
tested implementation SHA.
