# CAPY-PTP-003Q Report — composed private transport receiver

## Outcome

The remote-touchpad Adapter now owns a reusable
`PrivateTouchpadTransportReceiver<F>`. It validates the complete `CPTR` Hello
before calling a platform Sink factory, routes Data through the existing bounded
packet receiver, returns an Ack only after Sink submission, and makes malformed
transport, timeout, disconnect and Close terminal with bounded cleanup.

The Android ADB lab now uses this same state machine for both its default
synthetic projection and explicit `--vhf` projection. Its loopback bind, exact
binding, two desktop-input flags and VHF elevation requirement are unchanged.

## Tests

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_transport_receiver
cargo test -p capyio-remote-touchpad-adapter --bin capyio-ptp-adb-lab
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
```

The five new tests prove:

- Route/Stream epoch mismatch opens no Sink;
- mismatched Hello opens no Sink and faults the connection;
- active Data, release Data, both exact Acks and Close form one lifecycle;
- malformed Data submits no frame and closes the Sink;
- factory failure and active-contact timeout are terminal.

All targeted commands and full CI pass. No ignored hardware test was run by
these commands.

## Remaining work

The production Windows service still needs a reviewed local data pipe and
client-identity policy, a Runtime-owned Route provider and a service-hosted VHF
factory. Android still needs authenticated production transport and foreground
service lifecycle. Physical `003P` three-/four-contact evidence remains open.
