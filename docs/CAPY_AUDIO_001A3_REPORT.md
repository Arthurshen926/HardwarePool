# CAPY-AUDIO-001A3 Report

Date: 2026-08-24

Status: local implementation and authorized physical vertical slice complete

## Delivered

- generic Quick Action DTO schema version 1 with a stable action ID;
- truthful blocked/configured, Route lifecycle, epoch, evidence and Problem
  projection;
- finite `start`, `retry` and `stop` operations with unknown-field rejection;
- executable path, bind IP, port and endpoint ID retained in the trusted Tauri
  host environment and never accepted from or returned to the WebView;
- a 250 ms host-owned poll worker independent of WebView refresh;
- matching Tauri and visibly simulated/blocked Browser Mock contracts;
- physical Route exclusion from the ordinary demo Route toggle, preventing a
  synthetic command from bypassing the Quick Action controller.

## Automated evidence

- `cargo test -p capyio-desktop --lib`: 12 passed, 2 explicitly configured
  physical lab tests ignored by default;
- `cargo clippy -p capyio-desktop --all-targets -- -D warnings`: passed;
- frontend TypeScript checking and production Vite build: passed;
- `cargo xtask ci`: 108 non-desktop Rust tests passed, 2 real-CLI tests ignored,
  84 requirement IDs traced, manifests/repository/Adapter smoke passed, and
  frontend checking/build passed;
- Windows Tauri `cargo check` and `cargo build`: passed with only the normal
  MSVC import-library informational message.

## Physical evidence

The authorized lab used the official Audio Share v0.3.4 Windows ZIP whose
SHA-256 exactly matched
`d73ea0dc2e16e4cfcc52ae370a0f06ebaf9ac0a1f2b9b069e03b0c107cb37bc1`.
The CLI self-reported v0.3.4 and was explicitly bound to the host's Tailscale
address and an enumerated playback endpoint. The already installed Android
package was `io.github.mkckr0.audio_share_app`.

The real supervisor/Runtime test passed this sequence:

1. server start and stable receiver presence -> Route `Active`, epoch 1;
2. Android receiver stop -> typed receiver-loss Problem and `Offline`, epoch 2;
3. explicit retry -> later epoch 3 and a second `Active` receiver;
4. explicit stop -> Route `Stopped`, child reaped, no process/listener retained;
5. the independent active IMU Route remained `Active` throughout.

A second diagnostic run played `C:\Windows\Media\Alarm05.wav` through Windows
system playback. Android AudioFlinger identified the Audio Share process and a
48 kHz stereo Track with non-zero server frame count (`0x65586`). Its externally
automated disconnect click did not take effect before the 60-second test bound,
so that diagnostic run correctly failed rather than being counted as another
lifecycle pass. The first run is the retained lifecycle pass; the second adds
PCM-to-AudioTrack evidence only.

A later authorized repeat used the already firewall-approved copy of the same
hash-verified CLI. The Android receiver established TCP and UDP peer state, and
six consecutive system WAV plays advanced the active 48 kHz stereo Track from
zero to `0x24F326` server frames (2,421,542 frames). The Track was routed to the
Android speaker with zero-dB track gain. The user beside the receiver clearly
heard the phone playback. Explicit disconnect and process shutdown left no
listener on the configured port. A same-hash CLI launched from a different
temporary path could listen locally but did not accept the phone until the
previously firewall-approved executable path was used; Windows firewall access
is therefore treated as path-specific lab configuration, not binary-hash
authorization.

## Limits

TCP presence still does not authenticate the peer. Subjective audibility is
confirmed only for the repeat case above; it is not a latency, quality or soak
claim. Background/lock, audio focus, protected-content behavior, latency,
long-duration quality and production pairing/encryption remain Gate 7 work. No
upstream binary or APK is stored or distributed by CapyIO.
