# Private PipeWire camera helper

Original Apache-2.0 implementation. This is a dedicated media subprocess, not the
Adapter JSON-RPC control channel. It deliberately does not implement network,
consent, grants, session binding, discovery, camera capture, shell execution,
configuration files, or executable-path selection. Those belong to the owning
Runtime. Start only after Runtime authorization; stop by closing stdin or SIGTERM.
A submitted frame is not consumer-delivery evidence and must not activate a Route.

## Build

Requires Linux, a C11 compiler and installed development files for libpipewire-0.3
(PipeWire >= 0.3.49). No package installation is performed here.

    cc -std=gnu11 -Wall -Wextra $(pkg-config --cflags libpipewire-0.3) camera_helper.c $(pkg-config --libs libpipewire-0.3) -o capyio-linux-camera-helper
    cc -std=gnu11 -Wall -Wextra $(pkg-config --cflags libpipewire-0.3) camera_consumer.c $(pkg-config --libs libpipewire-0.3) -o camera-consumer
    cc -std=c11 -Wall -Wextra -Werror test_protocol.c -o test-protocol
    ./test-protocol
    cc -std=gnu11 -Wall -Wextra $(pkg-config --cflags libpipewire-0.3) test_helper.c $(pkg-config --libs libpipewire-0.3) -o test-helper
    ./test-helper

The helper main may be compiled as `-Dmain=capyio_camera_main` and called from a
fixed Cargo binary wrapper. Link its C object and the pkg-config libraries; do not
put a build-output executable path into user-controlled Runtime configuration.

## Private frame protocol

CLI exactly `--token TOKEN --lifetime-ms MS --deadline-unix-ms DEADLINE`. Token is 1..48 ASCII lowercase letters,
digits or hyphens, chosen uniquely by the owner, not a peer-provided name. MS is
1..300000 decimal, capped to the owner's remaining initial authorization and lease.
DEADLINE is the absolute initial grant/lease expiry in Unix milliseconds. The
helper rejects an already expired deadline, caps its initial monotonic lifetime
to the remaining wall-clock time and checks both clocks at ingress, process and
timer boundaries. Forward wall-clock jumps expire it; backward jumps cannot
extend the monotonic cap. The PipeWire node name is `capyio.camera.TOKEN`.

Inherited stdin must be a pipe (regular files and terminals are rejected) and contains complete records: 32-byte little-endian header followed
by exactly 1,382,400 bytes of contiguous NV12 (1280x720 Y then 1280x360 interleaved
UV, stride 1280), fixed 30/1 fps, progressive BT.709 limited range.

- 0..3: ASCII CPNV
- 4..5: u16 version 1
- 6..7: u16 header length 32
- 8..15: u64 sequence, strictly advancing, gaps allowed with discontinuity
- 16..23: u64 monotonic timestamp_ns, <= INT64_MAX, strictly advancing
- 24..27: u32 payload length 1382400
- 28..31: u32 flags; bit 0 discontinuity, no unknown bits

The helper does not trust producer timestamps for watchdogs. Local CLOCK_MONOTONIC
sets a 2-second whole-record assembly deadline, 5-second initial/idle deadline,
250ms completed-slot freshness and finite absolute lifetime. A complete newer
frame replaces the previous unconsumed frame, with discontinuity signaled; a
partially received record never replaces it. Each stdin callback reads at most
one frame's worth of bytes. Two fixed heap buffers are allocated before connect;
no processing callback performs IO or allocation. All callbacks use the main
loop, deliberately without RT_PROCESS. Graph buffers are negotiated as 2..3,
one contiguous block, with exact bytes and stride. No AUTOCONNECT flag or links.

Empty cycles carry an EMPTY chunk and GAP header, not stale video. EOF exits
immediately (a partial record is an error); SIGINT/SIGTERM, server/stream failure,
watchdogs and absolute expiry destroy the stream and connection. Other nodes are
never removed. Exit: 0 graceful/expiry; 2 malformed/truncated/timed-out media;
3 connection/setup/server failure; 4 format/buffer mismatch; 64 invalid CLI.

## Independent consumer evidence

Run `camera-consumer --token TOKEN --frames N` (1..300). It creates only
`capyio.camera-test.TOKEN`, an unlinked input. Use separate, explicitly invoked
local test tooling to enumerate the two exact node names and exact ports, then
link just those nodes with pw-link. Never infer port names or link wildcards.
Neither production helper nor consumer creates links or autoconnects. The
consumer exits after N verified buffers, 10 seconds with no real frame, or 30
seconds absolute, and emits a JSON result on stdout.

Produce the exact synthetic `camera_pattern(sequence, byte_index)` in
camera_protocol.h (Y in 16..235; distinct U/V in 16..240). The native consumer
compares all bytes, exact negotiated colors/format, chunk stride and size,
strictly increasing sequence and timestamp, and discontinuity for sequence gaps.
Empty graph cycles do not count. This is independent native byte evidence only
when actually compiled, explicitly linked, and run against a real local graph.
Protocol unit tests alone do not prove PipeWire delivery or camera selection by
third-party applications. Also test EOF, partial header/payload timeout, Stop,
server disconnect and expiry and verify the owned node disappears while an
unrelated test node survives. No user camera or personal device is needed.

## Dependency provenance

PipeWire is the optional native integration dependency (MIT-licensed API/runtime,
actively maintained upstream https://gitlab.freedesktop.org/pipewire/pipewire).
Prefer the system development package and pkg-config; no Rust crate dependency
or bundled PipeWire implementation. Alternative subprocess shell pipelines were
rejected because they widen configuration/executable attack surface. Official API
references: https://docs.pipewire.org/group__pw__stream.html and
https://docs.pipewire.org/group__spa__param.html . No upstream source was copied
into the helper. Locally downloaded SDK headers, if present for compile checking,
are test inputs only and must not be added to repository artifacts.

## Local validation (2026-10-21)

Portable protocol/pattern tests and real native callback tests pass. Helper and
consumer compile/link cleanly under `-std=gnu11 -Wall -Wextra -Werror` against
upstream tag 1.4.2 headers and the existing system 1.4.2 runtime. Twelve invalid
helper CLI cases return 64 and an expired absolute deadline returns 2. Native callback tests cover complete/partial records,
newest-slot replacement, sequence/timestamp replay, idle/partial/absolute timeout,
and EOF/truncated EOF. These tests create no graph nodes.

System development pkg-config files and headers are absent. For isolated compile
validation only, official source archive
https://codeload.github.com/PipeWire/pipewire/tar.gz/refs/tags/1.4.2 was downloaded;
no install occurred. The SDK inputs are not deliverables. A daemon, system config,
and protocol-native module are also absent, so live graph byte delivery and
node-lifecycle acceptance remain unverified here.

Subsequently, official Debian 1.4.2-1 packages `pipewire-bin` (amd64),
`libpipewire-0.3-modules` (amd64), and `libpipewire-0.3-common` (all) were downloaded
from https://deb.debian.org/debian/pool/main/p/pipewire/ and extracted under this
private test directory only. A single daemon attempt with a private socket name,
private config and no devices failed exit 255: protocol-native module loading
returned `Operation not permitted`. No socket was created. Testing stopped on
that denial; no escalation, global install, service change or namespace bypass
was attempted. Hosted Linux CI subsequently supplied the live graph evidence below.

The native callback test also passes UBSan (`-fsanitize=undefined
-fno-sanitize-recover=all`). Address/LeakSanitizer execution is not usable in this
executor: LeakSanitizer reports a fatal ptrace-related environment limitation.
No leak-clean claim is made. The renamed-main FFI object compiles with `-Werror`.


## Hosted acceptance (2026-10-21)

All four jobs passed at `23bcc2f00d21e5016ea221fee7a8c196264765ce` in
https://github.com/Arthurshen926/HardwarePool/actions/runs/39563035324 . The native
camera job passed all seven real graph scenarios, including a separate consumer
verifying 20 complete NV12 frames and the bounded fault/teardown cases described
above. This is software graph evidence using synthetic decoded input, not Android
AVC/network, physical camera or third-party application compatibility.

Ubuntu PipeWire 1.0.5 and the newer 1.4.2 headers both compile strictly. The private
camera test daemon alone gets a finite 16 MiB RLIMIT_FSIZE so the shared memfd can
hold three NV12 graph buffers; 2 MiB ordinary-output checks and audio limits remain
unchanged. `test_consumer.c` preserves success through normal cleanup while
retaining early stream failures and pixel-error failure outcomes.
