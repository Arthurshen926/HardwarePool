# CAPY-AUDIO-001B6B desktop/service control report

Date: 2026-08-28

Target: `DESKTOP-AT8EVE9` controlled local lab permitted by ADR 0029

Base commit under test: `b21d103e8400c547dd0a281a560899da521c38c4`

## Exact package and configuration

- service: `target/release/capyio-windows-service.exe`, 418,816 bytes,
  SHA-256 `CFBD97362DD8C02FED83878670B11DCCB62E41262BE083FA6208CAF11F82CC96`;
- Broker: `target/release/capyio-virtual-speaker.exe`, 236,032 bytes,
  SHA-256 `0ED5D96EC3B2A896920198BF5C36632B8CD2A413DA8F439F598D1C52AE8CA12D`;
- service name/account/start type: `CapyIOBroker`, LocalSystem, Automatic;
- fixed launch configuration: repository Broker path,
  `100.66.231.100:65530`;
- control pipe: `\\.\pipe\CapyIO.Broker.Control.v1`.

No driver package, signing certificate, boot configuration, Secure Boot,
BitLocker or Driver Verifier setting changed in this slice.

## Results

1. A normal, non-administrator desktop user read the service snapshot through
   the ACL-protected pipe. Ten queries before start, ten after stop and more
   than twenty after restart all succeeded; the previous one-use pipe failure
   is fixed.
2. `start` created the Broker listener and UDP endpoint. `stop` returned the
   Broker state to `stopped` and released both TCP and UDP port 65530 while the
   SCM service remained `Running`. A later start advanced generation 1 to 2.
3. The ignored physical desktop test
   `physical_windows_service_quick_action_controls_and_preserves_broker`
   passed. It proved that the Quick Action selects service mode without host
   environment variables, starts the Broker and deliberately preserves it
   when the desktop shuts down.
4. The already paired Android device was reconnected over ADB and its installed
   Audio Share receiver was launched. Eight consecutive snapshots reported
   `active`, generation 3 and `receiverPresent=true`; the established TCP peer
   was `100.66.157.119`.
5. The lab playback tool submitted five seconds of stereo 48 kHz audio to the
   default `CapyIO Speaker with Render Bridge`. Playback completed and the
   service still reported `active` with the receiver present. The operator had
   already confirmed audible phone output and working Windows endpoint volume
   control for this installed driver/Broker path.

## Current state and remaining release work

The automatic service, Broker and Android receiver are active at the end of the
run. CapyIO Desktop can now start and stop the service-owned speaker Route
without elevation; closing the UI does not terminate it.

This is a controlled-lab product flow, not a distributable release. Service and
driver installation/configuration are not yet owned by one signed installer,
the fixed workspace binary paths are not upgrade-safe, multi-user control
policy is unresolved, and the Audio Share-compatible Android transport still
lacks CapyIO pairing and application-layer security.
