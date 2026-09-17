# CapyIO display engine ABI version 2

Status: compile-tested native and Rust contract; real backend linked but feature-disabled
Decision: ADR 0053 and ADR 0054

## Boundary

`capyio_display_engine_v1.h` currently contains the frozen v1 declarations and
the active v2 callable boundary between the
CapyIO display Adapter and the Vibepollo-derived native engine. The host obtains
one size- and version-checked function table from
`capyio_display_engine_get_api_v2`. It must reject null functions, non-zero
reserved pointers or an unexpected structure size before calling `probe`.

All structures are fixed-size POD records. Paths, credentials, frame payloads
and ownership-neutral monitor selectors never cross the ABI. V2 adds one
64-byte inline, NUL-terminated UTF-8 field for the exact
`\\.\DISPLAYn` output that the Windows host has already resolved and verified
against the same capability identity. This transient OS binding is copied by
value; it is neither a pointer nor a persisted/user-provided selector. ABI v1
lacks this binding, remains fail-closed and is not loaded by the current host.

The four network fields are one GameStream namespace, not independent port
choices. Given the CapyIO-assigned Sunshine base, HTTPS is `base-5`, video is
`base+9`, control is `base+10` and RTSP is `base+21`. Native start reconstructs
the base and rejects a tuple that does not satisfy all four relationships.
ADR 0055 records the correction made before any feature-enabled v2 engine.

The production Rust loader does not accept that string from its caller. It
accepts the CapyIO driver's connected-monitor record and immediately joins the
record's target adapter LUID and target ID to two fresh `DisplayConfig`
inventories. Both observations must resolve to one active, available canonical
source with the requested source resolution and target rational refresh rate.
Only the feature-gated native fixture probe may bypass this ownership lookup.

## Lifecycle

| Host state | Allowed call | Successful next state | Failure behavior |
| --- | --- | --- | --- |
| unloaded | `get_api_v2` | API loaded | unload library |
| API loaded | `probe` | probed | reject this engine build |
| probed | `create` | ready | no handle may escape |
| ready | `start` | running | retain the handle for `destroy` |
| running | `request_key_frame` | running | report a bounded Route problem |
| running | `stop` | ready | timeout returns to running; other failures require destroy |
| ready/failed with a handle | `destroy` | destroyed | retain failed handle; do not unload |

`start` is not idempotent. A running handle accepts no second session. A new
Route epoch requires a successful stop followed by a new start record.

`stop` accepts 100–10,000 ms. `CAPYIO_ENGINE_OK` means capture, encoder,
network workers and callbacks are all quiescent before return. A timeout means
the session remains owned and running; the host may retry stop but must not
start another session or unload the library. No stop or destroy call may remove
a monitor whose capability identity differs from the start record.

`destroy` is legal only after successful stop or after a failed start. A failed
create returns no handle and permits immediate library unload.
Success invalidates the handle and guarantees no future callback. Failure keeps
the handle and library loaded so foreign worker code cannot execute from an
unmapped module. Destroy uses the same 100–10,000 ms timeout bounds as stop.

The native bridge copies the complete v2 start record into handle-owned storage
before atomically applying its exact-output and GameStream-base configuration
lease. A second acquisition fails with capacity rather than replacing the
active configuration. Any failed start clears that exact lease, restores the
prior process-local values and zeroes the copy before returning; destroy
repeats the cleanup.
The current feature-disabled bridge compiles and links the retained video
worker path but keeps its single readiness switch false until the Rust worker
and Android receiver lifecycle gates pass.

The native runtime now also compiles an internal staged launch-session plan.
It combines the exact v2 selection with nonzero per-connection key material,
requires encrypted RTSP and maps the Display Sink Capability identity onto the
private upstream client identity while forbidding audio, input and upstream
virtual-display ownership. This internal record is intentionally not appended
to frozen ABI v2. ADR 0056 supplies its authenticated handoff without changing
frozen ABI v2: the separate
`capyio_display_engine_start_authenticated_v1` symbol atomically receives the
v2 selection and a fixed 60-byte, pointer-free launch-material record. The
host requires the symbol, binds the material to the same Adapter and Display
Sink identities, never falls back to unauthenticated `start`, and wipes its
local material storage after the call. ADR 0072 makes the production bridge
raise that owner before listener accept, retain it through listener quiescence
and clear/wipe it after join. The readiness gate still returns `Unsupported`
before startup pending the matching worker and receiver paths.

The retained RTSP ANNOUNCE parser compares its raw requested width, height,
codec, dynamic range and bitrate plus normalized millihertz refresh against
that internal selection. A missing authorization or any mismatch returns
`403 Forbidden` before audio/video stream startup. Refresh normalization keeps
fractional modes such as 59.94 Hz by preferring `clientRefreshRateX100` when
`maxFPS` is not already expressed in millihertz.

## Callback contract

The engine may invoke `on_event` concurrently from native worker threads only
between successful create and successful stop/destroy. The callback context is
borrowed for that interval. Each 64-byte event repeats the Adapter and display
capability identities from the active start record.

The callback is real-time constrained: it performs only fixed-size validation
and a non-blocking bounded-queue attempt. It must not allocate, block, acquire a
contended lock, perform file or network I/O, call back into the engine, mutate
display topology or emit an ordinary log. Queue-full behavior is to increment
a fixed atomic dropped-event counter. `Fatal` uses a dedicated slot so ordinary
telemetry pressure cannot displace an unconsumed fatal event.

Event `sequence` is strictly increasing per successful start. Unknown kind or
status values, nil/mismatched identities, wrong ABI version and incorrect size
fail the Route closed. `value` is kind-specific numeric telemetry; it cannot be
interpreted for unknown event kinds.

## Status codes

Only codes 0 and -1 through -6 are valid. Unknown codes are ABI failure.
`CAPYIO_ENGINE_E_CAPACITY` reports encoder/session exhaustion without changing
display topology. `CAPYIO_ENGINE_E_TIMEOUT` has the special stop behavior above.
No error code authorizes global cleanup or retry with a different monitor.

## Loading and security

The eventual host must load an administrator-configured absolute path, verify
the pinned artifact hash before `LoadLibraryEx`, and use safe DLL search flags.
It must not search the current directory, `PATH`, a writable plugin directory
or a network share. Probe occurs before credentials, Route identity or monitor
activation are exposed to the engine.
