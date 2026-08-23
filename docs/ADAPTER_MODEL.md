# CapyIO Adapter Model

> Status: normative control/failure boundary for foundation Adapters.

## Adapter kinds

- `Physical`: local hardware or platform API.
- `Projection`: system device, route, injection or protocol outlet.
- `Connection`: existing vertical project or protocol.
- `Composite`: a multi-channel system such as game streaming or scrcpy.
- `Export`: Recorder, ROS, Foxglove, MCAP, USB/IP or local API.
- `Panel`: built-in visualization or interaction surface.

Kinds are descriptive; shared lifecycle and ownership are identical.

## Integration preference

```text
external Sidecar wrapper
  -> vendored vertical slice
    -> thin native Adapter
      -> new complete data plane/driver only when necessary
```

Upstream functions are not copied piecemeal into Core. A vendored slice retains
the upstream threading, buffering, protocol and lifecycle boundaries required
for correctness and records removed UI/updater/store code.

## Deployment

- `InProcess`: mobile/platform restriction or small trusted Adapter.
- `Sidecar`: preferred desktop isolation for substantial components.
- `ExternalService`: independently managed system or project.
- `DriverBacked`: minimal user-mode Adapter controls a separate driver boundary.

## Manifest

The versioned manifest declares ID, name, version, control protocol version,
kind, deployment modes, platforms, entrypoints, permissions, declared
Capability/Route Templates, integration mode, license metadata and upstream
metadata. Unknown required manifest versions fail explicitly.

Canonical JSON Schema: `protocol/schemas/adapter-manifest.schema.json`.

## Sidecar control contract

The first desktop contract is newline-delimited JSON request/response over
stdin/stdout. Each non-empty line is one complete bounded message. Logs use
stderr only.

Required methods:

```text
adapter.initialize
adapter.probe
adapter.catalog
adapter.health
route.prepare
route.start
route.stop
route.status
adapter.shutdown
```

Events use `adapter.event`, `capability.changed`, `route.state_changed` and
`diagnostic.reported`. Requests have a correlation ID; success has a result;
failure has stable code/message/retryability/data. Malformed or oversized lines
produce protocol errors rather than panics.

## Data-plane rule

The control channel never carries continuous audio, video, display frames or
high-rate sensor payloads. A Mock Adapter may send a small finite test sample,
which must be labeled a smoke-test channel rather than a production data plane.

## Failure isolation

Adapter Host owns child process handles, stdin/stdout/stderr tasks, deadlines
and bounded diagnostic retention. Unexpected exit produces a structured Problem
and fails only the Adapter's owned Routes. Shutdown is idempotent; orphaned
children are not left running after a completed smoke test.

## StandardPort vs AdapterManaged

- `StandardPort`: declared Profile/format is genuinely interoperable with other
  compatible CapyIO Ports.
- `AdapterManaged`: the Adapter owns end-to-end setup and data plane; Core
  controls lifecycle and UX only.

UI must label AdapterManaged limitations and must not imply arbitrary routing.

