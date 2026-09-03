# CAPY-GAMEPAD-006D — post-restart Windows Xbox 360 physical gate

Status: complete for Windows PnP/XInput enumeration, physical Android control
delivery, bounded owned-port liveness and exact cleanup.

Date: 2026-09-01

## Result

After the operator completed the installer-required Windows restart, the
authorized `DESKTOP-AT8EVE9` host reported both `usbip2_ude` and
`usbip2_filter` running. `ROOT\\USB\\0001` was Started with `oem172.inf`, the
fixed CLI remained `0.9.7.7`, and no imported USB/IP port existed before the
gate.

The pinned VIIPER v0.7.0 executable was started with SHA-256
`1868D682F4CC6D62349BBCCBF0727B05D3EB6E22027AC34F0F1D9B1DE56F2DDC`, both
automatic-attachment modes false, and API/USB listeners restricted to IPv4
loopback. The desktop preflight moved from zero exports to the unique exact
Xbox export `1-1` (`045e:028e`).

One bounded physical run accepted 15,285 valid Android touch+IMU frames,
observed non-neutral controls and finite motion, and submitted 14,980 VIIPER
states. The listener rejected 1,122 packets during the rebinding interval and
accepted none of those packets; the aggregate gate did not retain per-reason
rejection counts, so this report does not attribute them to a specific parser
condition. Replay count remained zero.

## Windows enumeration and live input

The one-shot usbip-win2 command returned owned port 1. While it was held,
`usbip port 1` reported:

```text
Port 01: device in use at Full Speed(12Mbps)
Microsoft Corp. : Xbox360 Controller (045e:028e)
usbip://127.0.0.1:3241/1-1
```

Windows PnP simultaneously reported Started devices for:

- `USB\\VID_045E&PID_028E` using `xusb22.inf` / `XnaComposite`;
- `USB\\VID_045E&PID_028E&IG_01` using `input.inf`;
- `HID\\VID_045E&PID_028E&IG_01` as a HID-compliant game controller.

A bounded native XInput slot-0 sampler ran while ADB held controls on the
foreground Controller Lab:

```text
A button: connected=true, non_neutral=true, buttons=0x1000
packet range: 161798..170580
left stick: max |LX|=26548, max |LY|=19223
packet range: 214706..223348
```

This proves live phone touch -> Android complete state -> VIIPER -> USB/IP ->
Windows xusb/XInput delivery. The separate DSU projection gate had already
proved that the same accepted phone source carries finite IMU. An attempted
third-party online tester page timed out in the in-app browser and is not
counted as evidence; the native XInput result is the retained consumer proof.

The first short attachment attempt began only ten seconds before its VIIPER
gate expired. The owned bus disappeared first, so the old lab learned about
loss only when detach returned “device not connected.” This was treated as a
lifecycle finding, not a successful enumeration result.

## Ownership hardening

The client now runs bounded `usbip port <owned-port>` after attach and requires
the exact port, loopback server, VIIPER bus and Xbox VID:PID before returning
ownership. The lab repeats that read-only check once per second during its hold.
Missing or mismatched inventory fails closed and still attempts detach of only
the returned port.

A post-change real run held port 1 for 15 seconds, passed every liveness check,
printed `CAPYIO_USBIP_ATTACHMENT_LIVENESS_PASSED`, detached exactly port 1 and
printed `CAPYIO_USBIP_ATTACHMENT_PASSED`.

The restart also made an existing DSU fixture startup race reproducible: its
first subscription datagram could be sent before bind and followed by a 100 ms
blocking receive, while the whole fixture completed in about 25 ms. The test
subscriber now uses bounded nonblocking 1 ms polling; production DSU behavior
is unchanged.

## Automated evidence

```text
cargo fmt --all -- --check                         PASS
git diff --check                                   PASS
cargo test -p capyio-windows-input -p capyio-desktop
                                                    PASS (55 passed, 5 physical tests ignored)
cargo clippy -p capyio-windows-input -p capyio-viiper-adapter \
  -p capyio-desktop --all-targets -- -D warnings   PASS
pnpm --dir apps/desktop build:web                  PASS
cargo xtask validate-docs                          PASS
cargo xtask validate-manifests                     PASS
cargo xtask ci                                     PASS
```

The first repository-CI attempt saw two unrelated Audio Share loopback binds
return transient Windows error 10013. Both focused tests passed immediately on
rerun without code or process changes, and the complete repository CI then
passed. This is retained as test-environment evidence rather than attributed to
the gamepad changes.

## Final state and limits

The final imported-port list is empty, the Xbox VID/PID is no longer connected,
the VIIPER-owned bus was removed, and the exact VIIPER process was stopped. No
driver, boot/security policy, persistent attachment or all-device detach was
changed.

The CapyIO desktop already visualizes the accepted phone controls/IMU and the
Windows preflight, but a continuously sampled native XInput echo is not yet a
production UI surface. Adding one directly to the current Rust crate would
require a reviewed platform FFI boundary because the crate forbids unsafe code;
this gate does not weaken that invariant.
