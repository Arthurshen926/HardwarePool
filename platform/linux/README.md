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

The catalog is empty by design. This host does not yet compose mock Routes, pair
with another device, persist an identity, open hardware, install a service or expose
a network listener. Existing demo commands remain explicitly Windows/Android fixtures.
Future Linux Adapters may use PipeWire, v4l2loopback, uinput/uhid and USB/IP; none of
those Linux hardware or system Projection implementations exists in this slice.
