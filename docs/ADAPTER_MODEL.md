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
kind, deployment modes, platforms, mode-specific bindings, permissions,
declared Capability/Route Templates, integration mode, license metadata and
upstream metadata. Unknown required manifest versions fail explicitly.

Manifest v2 is an intentionally breaking pre-alpha contract. `platforms` is the
union of supported platforms, while each declared deployment mode has exactly
one matching section under `mode_bindings`. Every top-level platform must be
covered by at least one mode-specific binding, and a binding cannot name an
undeclared platform. This permits one Adapter to use an Android `InProcess`
binding and desktop `Sidecar` bindings without inventing executables for the
mobile platform.

Mode-specific requirements are:

- `InProcess`: at least one platform binding with a non-empty module identifier,
  library identifier, or both;
- `Sidecar`: a direct executable entrypoint for every platform assigned to the
  Sidecar binding;
- `ExternalService`: a typed probe target and typed connection endpoint for
  every assigned platform;
- `DriverBacked`: a user-mode controller entrypoint/control interface plus
  driver dependency identifier, version requirement and interface metadata.

Entrypoints are opaque paths passed directly to a platform process API, not
shell command lines. Driver dependency records are descriptive prerequisites;
the manifest contains no installation command or privileged deployment action.
All manifest objects deny unknown fields so misspelled or unsupported required
semantics fail explicitly.

Rust semantic validation is canonical for cross-field rules such as mode/binding
and platform coverage. The committed JSON Schema is the distribution-facing
structural contract; repository tests check its version, required sections,
closed object shapes and committed examples against the Rust validator.

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

The Gate 5 SensorServer integration is an external-service protocol Adapter.
The independently installed Android app retains its WebSocket/JSON data plane;
CapyIO-authored code bounds and maps that documented message shape into an IMU
StandardPort. No upstream source or binary is linked or imported. WebSocket
connection lifecycle remains outside the parser/pairing contract and plain
`ws://` is a trusted-local-lab mechanism, not production transport security.

The Gate 7 Audio Share integration is an external-process `AdapterManaged`
boundary. CapyIO probes and later supervises a pinned, user-supplied `as-cmd`
executable through direct process arguments; it does not parse upstream logs as
Sidecar JSON-RPC. Audio Share retains TCP negotiation and UDP PCM delivery to
its independently installed Android receiver. This private contract is not a
`capyio.audio.frames/1` interoperability claim.

The Gate 8 MicYou integration is likewise an `AdapterManaged` compatibility
boundary. Its separately built Windows process and independently installed
Android application retain their private TCP/UDP PCM/Opus contract. The two
compatibility transports are physical regression baselines, not a shared wire.

ADR 0041 adds a common CapyIO-authored media seam above concrete audio
transports. ADR 0042 makes every mapping machine-checkable: backends declare
media access, supported encodings, field-by-field fidelity and security. Audio
Share now accepts a validated common PCM packet before deliberately stripping
unsupported metadata; MicYou declares an opaque external-process boundary and
has no common-packet API. Sharing the seam does not turn either private
AdapterManaged protocol into a StandardPort or make them interoperable.

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
