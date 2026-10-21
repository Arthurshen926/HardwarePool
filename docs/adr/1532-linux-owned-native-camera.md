# ADR 1532: finite native Linux PipeWire camera projection

Status: accepted implementation slice; hosted native-graph acceptance required.

## Decision

Add `platform/linux/capyio-camera` as a separate owner of already-decoded
`camera_720p30_nv12` frames: 1280x720, 30/1, NV12, BT.709 limited, 1,382,400 bytes.
It is not Android AVC decode/network integration, portal camera access, V4L2,
v4l2loopback, a kernel component or proof of third-party camera-app compatibility.
Existing audio paths, gates and default host launch behavior remain unchanged.

`RuntimeCamera` owns a real `NodeRuntime`. Before opening resources and every
poll/submit it validates the exact Session, Route, Stream binding, epoch, current
local Sink, negotiated format, finite grant and lease. Session readiness is not
cryptographic authentication: the caller must already have admitted its peer and
obtained consent. The owner creates no grant, lease, catalog or listener. Changed
identity/authority, malformed descriptor, replay, timestamp regression, wrong
length, EOF, expiry or helper failure is terminal. It never releases a later
Route epoch's lease. Stop/Drop destroy only its owned helper. Runtime mutations
are exposed for the existing local control owner; subsequent media always
revalidates them. The initial grant/lease deadline cannot be silently extended.

The optional `native-pipewire` feature builds a fixed-name sibling executable with
an original thin C/libpipewire implementation. Default workspace builds require
no native headers. A small Rust executable contains the sole FFI call; the owner
library forbids unsafe Rust. The helper must be an owned, non-group/world-writable
regular sibling. Its environment is cleared except a local socket name and user
runtime directory; no arbitrary executable, module or configuration path is an
API parameter. Existing installed PipeWire client defaults remain system policy.
Same-user arbitrary executable replacement is not a separate security boundary.

A private inherited pipe carries bounded binary CPNV records, not JSON control or
network data. A bounded one-frame Rust queue plus one writer bounds caller work;
backpressure drops complete frames and forces discontinuity. Partial records
have an absolute assembly deadline and never become visible. The helper has two
fixed frame buffers and fixed 2..3 graph buffers. All process callbacks run on its
main loop, without RT_PROCESS, allocation, blocking IPC or network work. Empty
cycles are EMPTY/GAP, never perpetual stale image repetition. A completed slot
expires after 250ms. Independent monotonic lifetime, absolute wall-clock grant/
lease deadline, idle/partial watchdogs, EOF and owned process teardown bound life.
The helper creates a unique native `Video/Source`, never AUTOCONNECTs, and never
removes an unrelated node. Helper cleanup has finite reap/writer deadlines.

Queued input and node registration do not prove delivery. This slice deliberately
leaves the Route Starting; future actual downstream delivery acknowledgement can
promote it without inventing readiness from a subprocess spawn.

## Dependencies and alternatives

Optional official distro libpipewire-0.3/libspa headers/runtime (MIT), pkg-config
and C compiler. PipeWire is actively maintained at
https://gitlab.freedesktop.org/pipewire/pipewire . No upstream implementation is
vendored. Direct C is smaller than adding a full Rust binding stack for this
bounded boundary. Shell pipelines and configurable executables/modules were
rejected. No installer, global service configuration or user-host deployment is
included; only disposable Ubuntu CI installs official development packages.

## Acceptance

Pure Rust tests cover exact Runtime/lease preflight, malformed/replayed frames,
expiry/revocation/crash, whole-frame overflow discontinuity, delayed open and
stale-owner cleanup. Native protocol and actual assembler callback tests cover
partial records, deadlines, newest-complete-frame replacement and EOF.

The explicit synthetic private-graph example uses the production Runtime owner,
not a demo source replacing the helper. The harness checks approval before node
creation, exact node -> Client -> PID -> PPid/executable ownership, no autolinks,
and explicitly links a separate native consumer. That consumer verifies every
NV12 byte, Y/U/V distinction, exact negotiated colorimetry, stride, sequence,
timestamps and discontinuities. Other scenarios force full-pipe backpressure,
no consumer, expiry, helper crash, server disconnect and malformed/partial IPC.
An unrelated sentinel must survive owned teardown; server-loss tests instead
verify a fresh graph has no stale reconnection, not old-server continuity.

Local cloud native compilation and callback tests passed. Actual cloud graph
startup was denied EPERM while loading protocol-native; no graph evidence was
obtained and no bypass was attempted. Hosted `linux-native-camera` is mandatory
before claiming native graph success. Android camera decode/network handoff and
physical/application compatibility remain separate later increments.
