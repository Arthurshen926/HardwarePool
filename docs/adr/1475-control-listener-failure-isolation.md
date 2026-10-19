# ADR 1475: Isolate Android control connection failures

Status: accepted bug fix

Date: 2026-09-18

## Evidence and mismatch

A user-visible Windows TLS timeout was reproduced against the tablet's actual
listening port. The Android foreground Service remained alive, but its control
accept thread had exited. The socket still listened and queued incoming TLS
bytes. The Java loop caught a per-peer RuntimeException outside the entire loop,
so one failed handshake could permanently disable new connections while the UI
advertised a listener. The original triggering exception was not retained in
observable diagnostics, so its exact cause is not asserted.

## Decision

Isolate accept/service failures per connection. An explicit native disconnect
operation drops and reconciles the owned link and associated touchpad jobs, but
preserves the listener, paired identity and bound port. Normal connection loss
also invokes this cleanup, including paths that returned early from dispatch.
The loop waits 250 ms between iterations and accepts subsequent peers. Failed
cleanup is fatal instead of retrying potentially inconsistent state. A dead
worker exposes neither a valid listen port nor active discovery/connection.

Mutual certificate pinning and resource authorization are unchanged. There is
no trust-on-error fallback, new dependency, protocol field, manifest permission,
driver change or firewall change.

## Tests

`:app:controlRecoveryContractTest` covers repeated rejected accepts, subsequent
success, dispatch exception, EOF, reconnection and fatal cleanup. Existing Rust
accepted-control tests and Android Lint/build remain required. Physical testing
must include invalid TLS followed by successful paired catalog/heartbeat on the
same port and repeated clean reconnection, not only initial TCP reachability.
