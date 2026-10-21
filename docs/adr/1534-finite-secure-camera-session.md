# ADR 1534: Finite paired camera session over pinned TLS

- Status: accepted policy/synthetic secure-session software slice; no physical-device claim
- Date: 2026-10-21

## Decision

Add a Linux-led single-peer library composition using the existing paired control
Session and credentials. It is not a new pairing/enrollment flow or a default
listener. A dedicated camera TLS 1.3 channel pins the exact existing peer leaf,
uses ALPN `capyio-camera/1` and magic `CPYCAM01`, and acknowledges the complete
Session/server/client/Route/Source/Sink/Stream/epoch/finite-grant binding before
media admission. Authentication alone does not grant resource access.

The distinct `RouteStatus.secure_camera_data` field 6 carries a nonzero actual
bound port, Stream UUID and explicit `Tls13MtlsV1` transport. No host is supplied:
the Source uses the authenticated control socket's peer address. Legacy field 5
`CameraDataOffer` still means plain CAVC on port 38173, including the existing
Windows lab path. A secure offer never falls back to that path. Unknown transport
versions and mixed offers are rejected. Existing touchpad/haptics/display framing
remains u16/176 bytes and existing listener reuse remains unchanged.

## Authority and lifecycle

`SecureCameraOwner` consumes an existing exact Ready control Session, Starting
CAVC Route, finite independent local grant and local Sink lease. Before binding
a listener it obtains a correlated authenticated Source lease response. The
Android Source remains governed by its actual shared resource-consent and lease
policy; no credential, offer or connection can manufacture consent. The receiver
checks local catalog endpoint availability, formats and the unchanged full Route.
A dynamic listener accepts one binding ACK and closes; no autoreconnect exists.
Decoder and projection helpers open only after data authentication and binding.

The explicit decoded-CAVC projection entry preserves the Route's negotiated
`cavc-avc-access-unit-v1` format, while accepting only validated canonical decoded
NV12 from the existing decoder. It does not mutate the Route to NV12 or loosen the
original direct-NV12 entry. Core remains free of transport and codec dependencies.

One transport thread handles bounded partial TLS reads while the owning thread
polls authenticated control. A rendezvous retains at most one complete record
awaiting consumption; no encoded access unit is dropped to relieve pressure.
Decoder IPC invokes the same control supervisor between progress attempts. Stop,
control EOF, exact-binding change, revoked/expired grants or leases, malformed
records, EOS, decoder failure and projection backpressure terminate the session,
close the channel and kill/reap the owned decoder/projection. No listener or
helper restarts automatically. Callers must continuously poll and eventually
stop/drop the owner; helpers retain independent finite deadlines.

All stages have absolute deadlines bounded by remaining grant, local/remote
leases and requested lifetime. Camera data frames use a u32 big-endian length,
checked against 4 MiB + 56 bytes before allocation. Inner CAVC parsing and the
existing decoder retain 32 MiB total input, 120 frames, at most 60 seconds,
canonical 720p30 rotation-zero Annex B, and all ADR 1533 codec restrictions.

## Android boundary and compatibility

The new Rust-owned sender consumes the existing active paired-directory entry
and credential bundle, deriving the exact certificate pin/server name and
socket-derived address. It reuses the real Android camera Source authority and
local lease policy. Complete CAVC records use the existing record guard and wire
contract, compatible with `AvcWireSessionEncoder`. Secure offers cannot be used
as plaintext Java capture configuration. The first proof supplies synthetic
encoded frames through actual Rust policy; Camera2 capture/JNI media plumbing,
physical permission, device installation and credential enrollment are not added
or claimed. The existing Java plain-Socket Windows lab sender remains explicitly
legacy and unchanged.

## Verification

Pure policy/protocol tests and actual loopback mTLS tests cover grant/lease/format
admission, exact bindings, wrong pins and ALPN, epoch/expiry, plaintext, oversize
length before allocation, slow handshake and partial-record cancellation. The
separate synthetic graph driver connects actual Android Rust policy and sender,
authenticated control/data, `RuntimeAvcCamera` and an independent native PipeWire
consumer. Private graph execution must be reported separately from compilation
and decoder-only tests. Hosted disposable CI is the authoritative graph gate;
no local IPC restriction is bypassed. Negative scenarios require owned process
and node cleanup. No kernel/V4L2/global daemon/network security changes occur.

## Relative lease receipt headroom

Source lease TTL is interpreted at the Source's receipt clock, not at the Sink's
request clock. Reserve one second from the minimum remaining grant, Sink lease
and owner lifetime before requesting it. If less than the protocol's one-second
minimum TTL remains after this reservation, fail closed before sending. The
same fixed one-second deadline bounds request/response completion, including
partial control responses. Recheck the fresh clock before accepting the lease.
No grant or lease check is relaxed; late receipts remain denied. The expiry
acceptance checks the authenticated actual Source lease deadline, which may be
earlier than consent expiry, rather than assuming both deadlines coincide.

## Hosted acceptance

Commit `e2be08dc1597aeedab4a7d0d111fd1fc24a8fc3b` passed all eleven actual secure
private-graph scenarios in [run 39570270828](https://github.com/Arthurshen926/HardwarePool/actions/runs/39570270828). The pixels case verifies
20 decoded NV12 frames with an independent native consumer; negative cases require
owned helper/decoder cleanup and the applicable exact verification markers. All
six jobs across the secure-camera, offline-AVC and existing Linux IPC workflows
were green at that commit. This acceptance combines actual Android Rust consent
and Source lease policy with synthetic encoded frames, pinned control/data and
the real Linux decoder/projection. It does not establish Camera2 app-to-Rust
bridge implementation or physical capture; that bridge is a separate increment.
