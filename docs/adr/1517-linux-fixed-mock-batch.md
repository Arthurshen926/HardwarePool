# ADR 1517: Fixed four-record Linux mock batch

Status: accepted for bounded implementation, 2026-10-19; batch IPC execution unverified.

## Decision

Add explicit `--mock-batch`, requiring `--mock-routes` and conflicting with
`--mock-payload`. ADR 1516's one-token mode remains unchanged. This new private
Unix-datagram fixture transfers exactly four synthetic records, sequences 0..3,
with exact per-sequence tokens and synthetic timestamps 1..4. No configurable
count, rate, endpoint, input file or payload is exposed. The distinct transport
label is `dev.capyio.test.unix-batch-v1`; this is not a public wire.

A stateful receiver binds Route, Profile, Stream and epoch, validates contiguous
sequence and exact timing before enqueue, and uses a capacity-two bounded queue.
Overflow rejects incoming data without eviction or sequence advancement. Each
normal receive is consumed immediately. The final receipt reports exactly four
consumed records, first/last sequence, total token bytes and an order-sensitive
FNV-1a diagnostic checksum over sequence (little-endian u64) followed by token
bytes. No partial receipt or Active state is published. This is trusted-fixture
integrity evidence, not authentication or flow-control interoperability.

Each source or sink Start uses one 500 ms I/O budget across its entire batch,
not a renewed per-record timeout. The host still starts Source before Sink;
a full kernel queue fails closed rather than growing memory or retrying. There
is no receiver thread, worker loop, continuous stream, background monitor or
asynchronous cancellation. Stop is handled after synchronous Start completes or
fails. Bind/connect, process wait and control-pipe writes retain ADR 1516's
limits; this is not a universal hostile-process deadline.

Records retain the 2048-byte bound and oversized-datagram detection. Existing
private-directory ownership, child-before-path cleanup, idempotent host Start,
receipt clearing on Stop and fresh Stream/epoch on restart remain mandatory.
Only the existing synthetic Profile, Basic QoS and trusted repository children
are used. No dependency, IP networking, hardware, credential, service, permission
or system setting changes occur. Forced host termination may leave residue.

## Validation

Socket-free tests cover ordered aggregation, gaps/replay/reorder, identity/timing,
corruption, partial completion and transactional queue overflow. Required hosted
IPC tests cover real four-record delivery, missing/partial peers, oversized data,
repeated Start, Stop/restart and owner cleanup on all supported exits. Existing
single-record tests and all eight CI guards remain required. Local socket
execution is prohibited by the known environment restriction; no retries or
escalation are part of this slice. Batch acceptance remains unverified until
exact-head hosted CI passes.
