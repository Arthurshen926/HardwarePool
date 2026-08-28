# CAPY-AUDIO-001B6A Windows service lab report

Date: 2026-08-28

Target: `DESKTOP-AT8EVE9` controlled local lab permitted by ADR 0029

Commit under test: `23f55f33d54bb8b9b011616a03c3713979ddb31b`

## Exact package

- service:
  `target/release/capyio-windows-service.exe`
  SHA-256 `52DE9AC17A79850DDEAB5CD112AFC0813C4199D30065A9B53A4B41EB67542E8F`;
- Broker:
  `target/release/capyio-virtual-speaker.exe`
  SHA-256 `0ED5D96EC3B2A896920198BF5C36632B8CD2A413DA8F439F598D1C52AE8CA12D`;
- service name: `CapyIOBroker`;
- account: `LocalSystem`;
- start type: manual/demand start;
- trusted launch configuration: explicit local Broker path,
  `100.66.231.100` and port `65530`.

No driver package, boot configuration, Secure Boot, BitLocker or Driver
Verifier setting changed during this run.

## Results

1. Preflight found no existing service, CapyIO process or TCP/UDP owner on
   port 65530.
2. The exact service was registered through an approved UAC elevation and
   reached SCM `Running` with exit code zero.
3. The LocalSystem service spawned one `capyio-virtual-speaker` child. That
   child owned the TCP listener and UDP endpoint on the explicit Tailscale
   address.
4. The independently installed Android Audio Share receiver established a TCP
   connection without the Tauri desktop process owning either lifecycle.
5. A controlled stop removed the service and Broker processes and released
   both TCP and UDP port 65530. A later start used new process IDs, restored
   the listener and the Android receiver reconnected automatically.
6. The directed Windows lab tool submitted five seconds of 48 kHz playback to
   the default `CapyIO Speaker with Render Bridge` endpoint after the service
   restart. The service, Broker, listener, established receiver and UDP endpoint
   remained present afterwards. This is submission/lifecycle evidence; a human
   audibility statement is recorded only after operator confirmation.

## Current state and rollback

The manual `CapyIOBroker` service is installed and running at the end of the
run. The desktop must not start its direct Broker fallback on the same port.

Approved rollback target is exactly `CapyIOBroker`:

```powershell
Stop-Service -Name CapyIOBroker
sc.exe delete CapyIOBroker
```

Deleting the service does not remove or change the already installed CapyIO
audio driver. The repository-built binaries remain ordinary workspace build
artifacts.

## Remaining gap

The service has no local management IPC, persisted product configuration or
installer integration. A normal desktop user therefore cannot yet query or
control its Broker state through CapyIO Desktop. B6B must add a bounded,
ACL-protected local control boundary before this becomes the normal product
flow.
