# CAPY-PTP-003B Report — Bounded Broker-to-VHF submission

Date: 2026-08-30

Status: complete (compile/static-validation only)

## Outcome

The unsigned KMDF/VHF driver now exposes an administrator-restricted,
exclusive device interface and accepts one canonical 50-byte buffered IOCTL
record at a time. It validates Hello/Data/Close state and submits bounded
hybrid Precision Touchpad reports through `VhfReadReportSubmit`. No driver was
installed and no device, service, certificate or security-policy change was
made.

## Kernel boundary

- device object and INF both apply protected SDDL granting full access only to
  LocalSystem and Built-in Administrators;
- the function device and entire installed stack are marked exclusive;
- a sequential passive-level WDF queue accepts only one `FILE_WRITE_DATA`
  IOCTL code;
- every request is exactly 50 bytes and carries magic, version, kind, payload
  length and contiguous sequence;
- Data is a complete snapshot of at most five active contacts; IDs are unique
  0..15, coordinates are 0..4095, flags/buttons are closed bit sets, and unused
  bytes must be zero;
- disappeared contacts are reported once with Tip clear and their last X/Y;
  replacement is split into at most two frames so neither reports more than
  five contacts;
- VHF failure poisons the open file session; later Data is rejected and file
  close performs one bounded best-effort release;
- the driver uses VHF's documented default buffering, which permits stack
  report buffers to be reused after `VhfReadReportSubmit` returns.

The driver still parses no network, Android, CapyIO Route, JSON or Protobuf
data and makes no gesture-policy decision.

## Certification response

Microsoft documents that Windows 10 does not require a signed PTPHQA blob but
does require the 256-byte certification-status feature to exist. The driver now
returns Microsoft's documented default pre-certification blob byte-for-byte.
Its SHA-256 is:

```text
b57b851d567808906f61a0122273da05c972140f1007355390aaf5dda8a072af
```

This is not a claim of HLK certification or Windows 8.1 compatibility.

## User-mode contract evidence

`capyio-windows-input` now has a platform-neutral record encoder with exact
Hello/Data/Close bytes, strict contact validation, transactional sequence
advance and canonical Ack validation. Three new tests cover fixed layout,
bounds/duplicate rejection and noncanonical Ack rejection.

## Commands and artifacts

```text
MSBuild CapyIOVhfTouchpad.vcxproj /m /t:Rebuild
  /p:Configuration=Debug /p:Platform=x64 /p:SignMode=Off
  /p:RunCodeAnalysis=true
  PASS, no warnings

InfVerif.exe /v CapyIOVhfTouchpad.inf
  INF is VALID

python scripts/validate_windows_touchpad_descriptor.py
  PASS (214 descriptor bytes, 256 certification bytes)

cargo test -p capyio-windows-input vhf_broker
  3 passed
```

Current artifact:

```text
drivers/windows-touchpad/x64/Debug/CapyIOVhfTouchpad.sys
size: 24064 bytes
SHA-256: 5BC76BDAF62FED4CE94779B888B604C3CED6D42139A4A4980D8F3C7E3BBE4CCE
Authenticode: NotSigned
```

## Remaining work

The Rust encoder is not yet connected to Windows device-interface enumeration
and `DeviceIoControl`, nor to the existing Runtime-admitted touchpad receiver.
Kernel behavior is compile/code-analysis validated but cannot be executed
without a separately approved driver deployment. The next code-only slice is a
user-mode Broker client with a fake transport, exact Win32 error mapping and
complete-frame projection tests. Only after that slice passes should an exact
unsigned lab package, recovery state and rollback command be presented for a
separate ADR 0029 deployment approval.

## References

- <https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-windows-precision-touchpad-collection>
- <https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/vhf/nf-vhf-vhfreadreportsubmit>
- <https://learn.microsoft.com/en-us/windows-hardware/drivers/wdf/controlling-device-access-in-kmdf-drivers>
