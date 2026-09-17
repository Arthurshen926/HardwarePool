# CapyIO Windows display sources and Broker

This user-mode crate projects a user-selected Windows Graphics Capture item
into `capyio.display.frames/1` and a capacity-one hardware-free Sink.

Production target selection is available only through the Windows system
picker with an explicit owner window. The capture border remains enabled. The
crate exposes no programmatic HWND/HMONITOR capture constructor and does not
request the borderless-capture capability.

Both Future and completion-callback picker entrypoints preserve the same opaque
target contract. The callback form lets UI frameworks start the picker on their
main thread without falsely marking HWND/temporary COM values as `Send`.

The free-threaded WGC callbacks set atomics only. D3D11 staging copy, map,
row-pitch removal and owned frame allocation run when the caller polls on its
worker thread. Size changes pause the Source until a newly negotiated exact
mode advances the stream epoch.

Automated tests use a deterministic fake backend. They never open the picker,
capture a real display or retain personal screen content.

The same crate also contains the user-mode side of the Windows virtual-monitor
lab. `IddFrameBrokerMapping` opens the fixed, versioned IDD frame mapping,
copies only validated newest-frame snapshots, converts BGRA to NV12 and feeds
the in-box Media Foundation H.264 encoder. Codec, TLS and reconnect policy stay
outside the UMDF/IDD driver.

`IddDeviceController` discovers exactly one present CapyIO 0.2.0 control
interface, opens it with read/write rights and strictly validates the fixed
query/list/connect/disconnect ABI. Zero or multiple interfaces, truncated or
trailing responses, identity mismatches, nonzero reserved fields and values
outside the eight-monitor bound fail closed. `capyio-display-control-lab`
exposes those four operations for controlled deployment evidence; it is not a
long-running topology coordinator. `IddMonitorCoordinator` builds the bounded
user-mode lifecycle transaction above it: verify a stable inventory, depart a
stale instance of the same Capability, own the v2 mapping before arrival, keep
the mapping on an uncertain disconnect, and expose explicit per-monitor or
ordered all-monitor shutdown. Its destructor performs no hidden fallible
IOCTL; callers use `disconnect_all`, while crash recovery starts from the
driver inventory. Automated tests use fake IOCTL and mapping backends and never
mutate an installed display device.

`capyio-display-lab-sender` is an evidence binary, not a production service. It
keeps one authenticated TLS 1.3 listener and the IDD mapping alive across
sequential Android receiver connections. Set the optional bounded
`CAPYIO_DISPLAY_MAX_CONNECTIONS` value to `1..=16` to exercise reconnect; its
default remains one connection. Every accepted socket performs fresh mutual
TLS and exact Session/Route/stream/epoch/format binding, and every connection
gets a new encoder pipeline so decoder configuration and a key-frame boundary
are emitted again. Process supervision, credential provisioning/rotation,
concurrent receivers and unbounded availability remain outside this lab
binary.

`capyio-display-topology-lab` is a Windows CCD/DisplayConfig evidence tool for
the virtual-monitor path. It resolves the exact adapter output by the closed
`Root\\CapyIOVirtualDisplay` and `CapyIO Phone Display` identity, then requires
exactly one available DisplayConfig path. `validate-extend` submits only the
currently active paths plus that CapyIO path to `SDC_VALIDATE`; `apply-extend`
uses the same closed set, saves it, immediately applies the exact
1980x912@60Hz mode while the path is active, and requires the CapyIO path to
remain active for a one-second stability window. It does not activate other
inactive virtual adapters. This lab tool does not replace Windows Settings, a
Runtime supervisor or isolated topology/rollback evidence.
