# CAPY-GAMEPAD-006C — desktop read-only Windows gamepad preflight

Status: complete before the installer-required Windows restart. No USB/IP
attachment, driver operation or restart was performed.

Date: 2026-08-31

## Result

The desktop Controller view now consumes a narrow, fixed-configuration host
preflight instead of displaying only a placeholder Windows projection state.
The WebView can request one parameter-free read-only refresh. The Rust host:

- probes only VIIPER `0.7.0` at `127.0.0.1:3242`;
- launches only `C:\Program Files\USBip\usbip.exe` and requires exact version
  `0.9.7.7`;
- lists only the USB/IP server at `127.0.0.1:3241`;
- returns only exact Xbox 360 `045e:028e` matches instead of exposing a generic
  USB inventory;
- reports `offline`, `host_gate_required`, `export_ready` or `failed` with
  stable sanitized diagnostics;
- rejects multiple matching exports as ambiguous;
- starts no VIIPER or persistent/background helper; the only child operations
  are bounded fixed `usbip.exe --version` and `list` calls;
- never creates a VIIPER bus, attaches/detaches USB/IP, installs/removes a
  driver, restarts Windows or changes boot policy.

The DTO now exposes separate VIIPER/USB-IP readiness, matching-export count,
the unique bus ID when present, owned USB/IP port, last event and problem code.
`ownedUsbipPort` remains empty until the separately gated post-restart Route
actually owns an attachment.

The Tauri command is single-flight and executes the bounded probe on a blocking
worker without holding the shared gamepad-state mutex. Android/DSU state
polling and neutralization therefore do not wait on child-process or socket
deadlines; only the completed immutable projection snapshot is published under
the short-lived lock.

## Real no-restart evidence

The pinned VIIPER executable used for the run had SHA-256
`1868D682F4CC6D62349BBCCBF0727B05D3EB6E22027AC34F0F1D9B1DE56F2DDC` and was
started hidden with both automatic-attachment modes explicitly false and both
listeners bound to IPv4 loopback.

With VIIPER and usbip-win2 available but no owned bus, the same host preflight
used by the Tauri command reported:

```text
status=host_gate_required
viiper_ready=true
usbip_ready=true
export_count=0
bus_id=none
owned_port=none
last=read_only_preflight_no_export
```

During a bounded VIIPER-only gate, twelve valid complete Android-shaped states
were accepted. The gate observed non-neutral controls, finite IMU values and a
peer-timeout neutral, while the read-only desktop preflight concurrently
reported:

```text
status=export_ready
viiper_ready=true
usbip_ready=true
export_count=1
bus_id=1-1
owned_port=none
last=read_only_preflight_passed
```

After the gate removed its owned VIIPER bus, preflight returned to
`host_gate_required` with zero exports. After the exact VIIPER process was
stopped, it reported `offline` and stable code
`CAPY.GAMEPAD.VIIPER_UNAVAILABLE`. This covers ready, disappeared-resource and
external-service-offline transitions without USB/IP mutation.

## UI and automated evidence

- Browser Mock implements the same DTO, reports `unsupported` and keeps the
  read-only button disabled rather than claiming platform access.
- The production web build and TypeScript contract pass.
- Default-width and 844 x 390 in-app browser checks show no horizontal overflow
  in the page or the expanded Windows status card and no console warnings or
  errors.
- usbip-win2 fixtures prove the read-only inventory filters unrelated devices
  and executes only exact version plus list arguments.
- desktop mapping fixtures cover no export, one export, ambiguous exports and
  dependency failures with partial readiness and sanitized Problems.
- a single-flight fixture proves concurrent invocation is rejected and the
  permit is released on every scope exit.

The final repository checks were:

```text
cargo fmt --all -- --check                         PASS
git diff --check                                   PASS
cargo test -p capyio-windows-input -p capyio-desktop
                                                    PASS (53 passed, 5 physical tests ignored)
cargo clippy -p capyio-windows-input -p capyio-viiper-adapter \
  -p capyio-desktop --all-targets -- -D warnings   PASS
pnpm --dir apps/desktop build:web                  PASS
cargo xtask validate-docs                          PASS
cargo xtask validate-manifests                     PASS
cargo xtask ci                                     PASS
```

After all live evidence was complete and the exact VIIPER process had been
stopped, the rebuilt desktop debug entry point was run once more. It returned
exit code 2 by design, with `status=offline`, zero exports, no owned port and
`CAPY.GAMEPAD.VIIPER_UNAVAILABLE`. That is a negative-state acceptance result,
not a build failure. The only build diagnostic was the Rust toolchain relaying
the normal MSVC linker message that it created the desktop DLL import library;
Clippy with warnings denied remained clean.

## Remaining physical gate

The installer-required restart still blocks only the mutating system
Projection proof: exact one-shot attach, Windows PnP/XInput/browser enumeration,
phone-control observation, exact-port detach and final disappearance. The
read-only preflight deliberately cannot bypass or assert completion of that
gate.
