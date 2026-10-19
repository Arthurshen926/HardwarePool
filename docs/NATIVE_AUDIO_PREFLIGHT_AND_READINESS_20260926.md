# Native audio preflight and readiness corrections

Date: 2026-09-26. Scope: local source changes and automated tests only. No service,
driver, device permission, or installed application was changed by this slice.

## Changes

- `capyio-windows-service` rejects non-unicast bind addresses and conflicting
  speaker/microphone local or peer UDP endpoints during configuration parsing,
  before constructing or starting broker children. Equal port numbers remain
  valid when their IP addresses differ. Both directions own separate explicit-peer
  UDP sockets; this does not add multiplexing or automatic endpoint negotiation.
- The desktop audio boundary distinguishes service transport readiness from
  observed TCP receiver presence. A native service reporting `active` with
  `receiverPresent=false` no longer becomes a fabricated TCP connection.
- Native transport readiness may still activate the existing adapter-managed
  Route. The Quick Action reports `process_and_route_state` and explains that
  remote receipt/playback is unverified. Only a full stable TCP observation
  sequence reports `stable_tcp_receiver_presence`; preceding native transport
  polls do not count toward that evidence.
- Existing explicit Stop and service survival after closing the desktop window
  remain unchanged. No UI DTO schema expansion or new dependency was introduced.

## Automated evidence

| Command | Result |
| --- | --- |
| `cargo test --offline -p capyio-windows-service --lib` | 82 passed, 2 ignored subprocess fixtures |
| `cargo test --offline --manifest-path apps/desktop/src-tauri/Cargo.toml --lib audio_share_runtime::tests` | 14 passed, 2 physical tests ignored |
| `cargo test --offline --manifest-path apps/desktop/src-tauri/Cargo.toml --lib quick_actions::tests` | 8 passed, 1 physical test ignored |
| `git diff --check` | Passed |

Eight tests were added: four preflight tests (including six non-unicast cases),
two Route/evidence tests, and two service snapshot tests (including ten
state/presence combinations). Tests do not start the installed Windows service
or access Android hardware. Service subprocess fixtures are exercised by their
parent tests; directly running their fixture entry points is intentionally
ignored. The first service test run exposed a test-only incorrect mode spelling;
the corrected suite above passed.

## Delivery and remaining acceptance

The endpoint guards require rebuilding and explicitly deploying the updated
`capyio-windows-service.exe`; rebuilding the desktop alone does not change an installed
service. The desktop readiness correction works with the existing service
snapshot contract after the desktop is rebuilt. No deployment is claimed here.

These tests do not prove audio reached a real consumer. Remaining checks include:

1. Confirm matching explicit native audio endpoints on Windows and Android,
   start the Android speaker sink, select the CapyIO virtual speaker in a real
   Windows playback application, and confirm audible playback plus advancing
   Android receive/playback counters.
2. Start the Android microphone source, select the CapyIO virtual microphone in
   a real Windows recording/call application, and verify capture and clean stop.
3. Exercise simultaneous microphone/speaker use, independent peer loss, restart,
   and port release. Check that service process liveness alone never appears as
   evidence of successful playback or recording.

Unified Route-negotiated audio endpoints, encrypted product audio transport, and
ordinary-user driver/service installation are outside this slice. Existing lab
UDP configuration must not be presented as completed product interoperability.
