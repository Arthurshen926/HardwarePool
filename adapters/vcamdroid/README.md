# VCamdroid camera Adapter boundary

Candidate camera vertical integration behind an isolated Connection/Projection
boundary. No upstream VCamdroid source, FFmpeg/RTSP component or virtual-camera
registration is present.

The crate owns the bounded private CAVC v1 record decoder/stream guard plus a
Windows-only Media Foundation H.264-to-NV12 decoder. The lab executable accepts
one ADB-reverse loopback connection by default; an explicit bounded reconnect
grace supports full-close Android camera restarts. It can report encoded counters or,
with `--decode-nv12`, bounded decoded-frame byte counts and FNV-1a checksums.
It stores no frame bytes. Explicit `--publish-shared` and
`--publish-shared-local-lab` modes can hand decoded canonical frames to the
camera host's fixed Global production mapping or fixed Local lab mapping. The
Global owner requires a suitable host token; the Local mapping is never opened
by registered COM activation. This must not be described as a working
VCamdroid integration, production transport or visible virtual camera until
the remaining controlled system roundtrip passes.

CAPY-CAMERA-001C9 enables `CODECAPI_AVLowLatencyMode` on the inbox Windows H.264
decoder and verifies the value through `ICodecAPI` before accepting it. The lab
reports that state and its maximum pending decoder sample count. Two controlled
V2419A streams read the mode back as enabled and observed no pending-sample
backlog; this is decoder-stage evidence, not a glass-to-glass latency claim.

CAPY-CAMERA-001C10 adds bounded same-clock latency aggregates. The decoder
records submit-to-copied-NV12 count, average and maximum without retaining a
timing series; the shared publisher separately records NV12 conversion plus
mapping publication. A 300-frame V2419A Local-lab run measured about 2.4 ms
average decoder time and 0.137 ms average publication time. Android, transport,
Frame Server and display time remain outside these values.

CAPY-CAMERA-001C22 restores blocking mode on every replacement TCP stream
accepted while the listener is temporarily nonblocking. This is required on
Windows, where the accepted socket can otherwise inherit nonblocking mode and
turn an ordinary delayed first byte into `WSAEWOULDBLOCK` (`10035`). The normal
read path retains its existing 15-second timeout and all other errors remain
fail-closed.

CAPY-CAMERA-001C23 adds explicit `--trusted-lan-bind` and
`--trusted-lan-peer` options. They must appear together, use different canonical
IPv4 literals inside RFC1918, link-local or 100.64.0.0/10 space, and retain the
fixed TCP port 38173. The listener binds only the named interface and closes
every connection whose source address is not the exact peer before parsing a
record. Wildcard bind, DNS, public addresses and arbitrary ports are not
representable. This is a plaintext trusted-lab escape from ADB, not a production
authenticated transport.

CAPY-CAMERA-001C24 extends only the fixed post-connection replacement-stream
grace from 10 to 60 seconds. The duration remains compiled in and bounded;
callers cannot select it. A reconnect at exactly 60,000 ms is accepted and one
at 60,001 ms is rejected. Peer admission, record validation, normal read
timeouts and fail-closed behavior are unchanged.
