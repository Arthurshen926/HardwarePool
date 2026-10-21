# ADR 1536: Explicit finite Linux camera launcher

- Status: accepted software composition; physical acceptance deferred
- Date: 2026-10-21

## Decision

Add the opt-in `capyio-node linux-camera --trusted-lab` executable entry. The
legacy playback/capture CLI and default headless host are unchanged. This entry
composes the existing secure camera owner, not an example or an alternate codec,
permission, pairing or transport implementation. `--trusted-lab` acknowledges the
experimental finite projection; unlike legacy audio, camera media is pinned mTLS.

Only a trusted local launcher with exclusive inherited FIFO stdin/stdout can
supply bootstrap or approval. The framing and nonblocking bounds are reused from
ADR 1522: u32 big-endian length followed by the exact payload, three bootstrap
frames within five seconds. They are, in order:

1. Strict JSON object `{version:1,node_id,source,listen,duration_secs}`. `source`
   is the exact configured Android Node/Capability/Port reference. `listen` is an
   IPv4 loopback, RFC1918 or link-local socket, with zero allowed for an ephemeral
   control port. `duration_secs` is 3 through 60. Extra/duplicate fields fail.
2. Existing versioned paired-directory JSON with exactly one active peer matching
   the configured Source Node and no other directory entries.
3. Existing binary `ControlCredentialBundle`, never arguments, paths, environment
   variables, logs or persistent stores. Encoded secret buffers are cleared on
   drop using the existing best-effort mechanism; this does not isolate a process
   from its own trusted invoker or promise compiler/swap/process-inspection safety.

There is no arbitrary helper path, camera picker, incoming Route admission,
password argument, automatic credential enrollment or implicit Android capture.
The process accepts one pinned control connection within 15 seconds, then drops
its listener. It exchanges real authenticated catalogs within five seconds and
publishes one ephemeral `LinuxCameraSink` CAVC Sink. The exact selected published
Android Camera Source must be Available, AdapterManaged, Interactive and support
`capyio.video.frames/1` with `cavc-avc-access-unit-v1`. Configuration is never used
to fabricate the remote catalog. Catalog changes during consent are revalidated.

## Independent authorization and finite ownership

An `approval_required` event identifies fresh request/Route/Session IDs, exact
Source/Sink, format and finite limits. Only an object with version 1, command
`approve`, and those exact request/Route IDs is accepted within 30 seconds.
`status` and `stop` have no IDs. Unknown commands, mismatched approvals, input
EOF, oversized/partial records and timeouts fail closed. No media listener or
helper is created before local approval or merely because a catalog was shared.

After approval Linux sends a Pending Draft. Android independently obtains its
existing camera resource consent and sends a finite grant. Linux uses the minimum
of both deadlines and requires the peer to acknowledge that exact finite Draft.
It acquires its exclusive Sink lease and prepares the canonical Starting Route.
`SecureCameraOwner` obtains the correlated authenticated Source lease before
publishing a distinct secure camera offer at a dynamically bound port on the
configured local IP. Actual data mTLS and exact full binding precede both fixed
native helper processes. No grant/lease is automatically renewed, and no rejected
or ended session reconnects.

The existing owner checks control EOF/Stop, catalog revoke, unchanged exact Route,
expiry and both leases. Its decoder supervises remote control while blocked; the
outer launcher additionally polls local Stop/EOF between bounded owner operations.
All exits stop/drop the owned control/data/decoder/projection. Helper IPC and
independent native lifetimes remain bounded even on launcher failure. No kernel,
V4L2, uinput, global daemon, default routing or security setting is touched.

## Packaging and honest state

Build `capyio-node --features native-camera`, forwarding to the existing
`capyio-linux-avc/native-pipewire` feature, and build both helper bins separately.
Install `capyio-node`, `capyio-linux-avc-helper` and
`capyio-linux-camera-helper` in the same owner-controlled directory. Existing
production launchers reject symlinks/non-regular helpers, mismatched owners and
group/world-writable helper files; no PATH lookup or shell execution is used.
The feature alone does not cause Cargo to package helper binaries.

The canonical decoder contract remains 1280×720, 30 fps, rotation 0, Annex B,
32 MiB total encoded input, 120 frames and at most 60 seconds. Queued output is
not consumer proof: `status`, `camera_bound` and `camera_record` report the real
Starting state, child IDs, node name and queued count with
`consumer_delivery_verified:false`. They never promote Active. An independent
pixel consumer remains required for software graph acceptance, and physical
Camera2, third-party applications and device installation remain deferred.

## Verification

Required subprocess tests invoke the actual executable via inherited private
pipes and actual Android Rust camera policy plus pinned control/data channels.
They cover config/approval/source/pin/lease rejection, no child before consent,
finite Stop/EOF/timeout cleanup, and unchanged audio behavior. A separate required
private PipeWire test checks synthetic CAVC through production decoding, exact
helper parentage, independent pixels and teardown. Private-graph execution is
reported separately from compilation and portable subprocess tests; no restricted
local socket or daemon operation is bypassed. Existing CI jobs remain mandatory.
