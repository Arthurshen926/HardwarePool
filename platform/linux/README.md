# Linux platform boundary

`capyio-host` owns a hardware-free Linux NodeRuntime through bounded inherited-pipe
management. Start with an explicitly supplied stable identity:

```text
cargo run -p capyio-node -- linux-host --node-id 00000000-0000-4000-8000-000000000301
```

Send newline-delimited requests on stdin; read one JSON response per record:

```json
{"version":1,"id":1,"command":"host_status"}
{"version":1,"id":2,"command":"get_snapshot"}
{"version":1,"id":3,"command":"host_shutdown"}
```

stdout is protocol only; logs use stderr. EOF exits. An oversized request terminates
without waiting for newline. See ADR 1514 for limits, error codes and trust boundary.

The default catalog is empty by design. Without explicit mock opt-in this host
does not compose Routes. It does not pair with another device, persist an identity, open hardware, install a service or expose
a network listener. Existing demo commands remain explicitly Windows/Android fixtures.
Future Linux Adapters may use PipeWire, v4l2loopback, uinput/uhid and USB/IP; none of
those Linux hardware or system Projection implementations exists in this slice.

## Opt-in mock subprocess lifecycle

ADR 1515 adds a separate hardware-free test mode:

```text
cargo build -p capyio-node -p capyio-mock-source -p capyio-mock-sink
cargo run -p capyio-node -- linux-host --node-id 00000000-0000-4000-8000-000000000301 --mock-routes
```

Only repository-built `capyio-mock-source` and `capyio-mock-sink` beside the node
executable are launched, without shell/arguments. That directory must be trusted;
Adapter ID checks do not authenticate executable content. The Source catalog is
local; the Sink belongs to a clearly synthetic peer with synthetic Session/consent.

```json
{"version":1,"id":1,"command":"mock_route_start"}
{"version":1,"id":2,"command":"get_snapshot"}
{"version":1,"id":3,"command":"mock_route_stop"}
{"version":1,"id":4,"command":"host_shutdown"}
```

`mock_source_crash` intentionally exits only the owned mock Source for failure
exercises. The next snapshot/status or repeated Start observes failure, stops and
reaps the mock pair; the host remains queryable. Restart the host to recover a
failed mock pair. Observation is command-boundary, not a background watchdog.

These are real subprocesses controlling a simulated Route lifecycle, not data
transfer. `sample_transfer_enabled`, `hardware_enabled` and `network_enabled` stay
false. All resources retain `test_only` labeling. No real peer trust is created.

### Experimental finite payload fixture (ADR 1516)

After building the node and both mock binaries, add `--mock-routes --mock-payload`
to `linux-host`. Start sends one synthetic UTF-8 token over separate Unix-local IPC;
Active requires a Sink receipt. Repeat Start does not resend; Stop clears receipt;
restart uses a fresh Stream/epoch. Without the additional flag, lifecycle-only
behavior is unchanged. This needs Linux Unix sockets, not IP listeners or hardware.
IPC execution remains unverified until the required full tests pass.
