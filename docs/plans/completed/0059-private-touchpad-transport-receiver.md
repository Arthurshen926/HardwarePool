# CAPY-PTP-003Q — composed private transport receiver

Status: completed

## Goal

Create one hardware-free lifecycle that validates private transport records,
opens a selected Sink only after exact Hello binding, acknowledges Data only
after Sink acceptance and closes on every terminal path. Reuse it in the Android
ADB lab without changing the synthetic-default or explicit-VHF safety gates.

## Acceptance criteria

- Route/Stream mismatch and invalid receiver bounds fail before factory open.
- Mismatched Hello permanently fails with factory open count zero.
- Valid Hello, active/release Data, exact Ack and Close form one lifecycle.
- Malformed Data closes the active Sink and no frame is submitted.
- Factory failure and active-contact timeout are terminal and typed.
- The ADB lab selects synthetic by default, selects VHF only with `--vhf`, and
  uses the composed receiver for Hello/Data/Ack/Close.
- Targeted tests, strict Clippy and full repository CI pass.

## Result

All criteria passed. Five new state-machine tests are hardware-free; the ADB lab
argument test remains device-free. No driver interface, socket, APK or Windows
service is opened by default tests.

Evidence: `docs/CAPY_PTP_003Q_REPORT.md`.
