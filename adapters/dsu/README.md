# DSU Adapter

CAPY-GAMEPAD-001A implements a transport-free DSU v1001 protocol boundary for
normalized CapyIO IMU samples. It provides bounded request parsing, table-free
CRC-32, explicit signed axis permutations, SI-to-DSU unit conversion and fixed
100-byte pad-data encoding with neutral `capyio-input` controls.

CAPY-GAMEPAD-001B adds a caller-polled UDP endpoint restricted to IPv4 loopback.
It preallocates one maximum-size UDP receive buffer, keeps at most 16 subscribers
in fixed storage, expires registrations using caller-supplied monotonic time and
limits receive work per poll. Port `0` selects an OS-assigned test port; the
crate never binds the conventional port automatically. Malformed requests and a
full subscriber registry are observable without terminating unrelated clients.
On Windows, a datagram sent to a subscriber that has just closed can make the
next nonblocking receive report `ConnectionReset`; the endpoint treats this as
an idle receive so one stale UDP peer cannot terminate the DSU Worker.

CAPY-GAMEPAD-001C adds a caller-owned background worker around that endpoint.
The worker accepts one fixed `capyio.motion.imu-samples/1` stream epoch through
a bounded non-blocking queue, rejects wrong/stale/future or late samples, keeps
gap and transport counters, and maps accepted envelopes to DSU without coupling
to SensorServer or the desktop UI. Stop is idempotent and joins the worker
thread; a new Route epoch starts a new worker.

CAPY-GAMEPAD-002A adds deterministic normalized-control projection. Buttons,
D-pad, sticks and triggers can now share the same 100-byte DSU packet as the
existing IMU motion sample. Source axis signs are explicit, and callers choose
either the pinned protocol's Y/B/A/X face-field names or Dolphin's current
DualShock physical interpretation. Unsupported paddle buttons fail closed.
The CAPY-GAMEPAD-001A neutral-only encoder and `publish_motion` remain as
compatibility entry points.

CAPY-GAMEPAD-002B extends the optional worker with an explicit dual-input mode.
`start_with_controls` accepts separate fixed-epoch IMU and complete-gamepad
anchors, and exposes independent non-blocking senders. Each valid update emits
the latest motion plus latest controls; controls arriving before the first IMU
sample are cached without fabricating motion. The existing `start` entry point
remains IMU-only.

The controls stream has its own sequence tracker. A controls gap emits neutral
before the later complete state, while an IMU gap does not invalidate otherwise
healthy controls. Invalid or DSU-unrepresentable controls, future control
epochs, sequence exhaustion, explicit lifecycle neutral requests and worker
stop all clear cached controls. Queue overflow advances a generation so older
queued controls cannot reactivate after the fail-safe transition. The Runtime
owner can call `request_neutral` for Route offline/failed/stopped or upstream
peer-loss signals; this Adapter does not infer those signals from DSU client
subscription expiry. The request is non-blocking and is applied by the worker
within a bounded scheduling cycle; it is not an acknowledgement or a latched
offline state. A lifecycle owner must stop its producer before requesting
neutral if later snapshots must not reactivate controls.

CAPY-GAMEPAD-002C separates the high-rate IMU and complete-controls ingress
queues. Motion pressure can no longer consume controls capacity, and controls
pressure can no longer reject motion. Both queues remain bounded and
non-blocking for producers. The worker polls them without a new dependency and
processes at most 16 items from each stream per scheduling cycle, guaranteeing
equal progress under sustained load before returning to DSU polling and stop
checks. `queue_capacity` remains the compatible motion-queue setting;
`controls_queue_capacity` configures the independent controls queue.

CAPY-GAMEPAD-003A adds a platform-neutral `GamepadStateComposer` in
`capyio-input`. A local source can apply already-normalized semantic button,
D-pad, stick, trigger and reset updates while emitting a complete state on every
successful update. The composer fixes one stream/epoch, accepts caller-owned
timestamps, validates before committing and keeps its valid hot path
allocation-free. Its non-consuming `anchor` can establish the dual worker's
expected first sequence. Touch geometry, pointer ownership, multi-touch
arbitration, coordinate transforms and Android lifecycle remain source/UI
policy rather than part of this helper or the DSU Adapter.

The slice exposes only logical slot 0. Port-info requests for the other valid
DSU slots (1 through 3) produce disconnected responses, while pad-data encoding
for those slots fails explicitly as unavailable.

The low-level endpoint has no background thread, while the optional worker is
owned explicitly by its caller and has no authority over Route/session state.
Neither layer claims emulator interoperability, accesses Android sensors,
accepts remote-network clients, installs a driver or chooses a physical phone
mounting. `CAPY-GAMEPAD-001D` now binds Worker start/stop/failure to one
Runtime-owned `ExternalProtocol` IMU Route in `capyio-windows-input`; `001E`
provides a bounded SensorServer-to-DSU physical-lab command and requires
observed Cemu/Dolphin subscription/delivery evidence before reporting success.
Real emulator behavior and axis calibration remain manual lab evidence rather
than a codec-test claim.
The dual-input mode is likewise a bounded integration boundary, not a Runtime
Route owner or an Android touch-control implementation.
