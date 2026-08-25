# CAPY-AUDIO-001A4 Report

Date: 2026-08-25

Status: local implementation and current Windows inventory probe complete

## Delivered

- manual rescan and playback-endpoint picker on the remote-speaker Quick Action;
- bounded endpoint display names with control-character replacement;
- per-scan opaque selection tokens resolved only inside the trusted Rust host;
- rejection of unknown, stale, malformed and overlong selection tokens;
- endpoint replacement only while the Runtime Route is inactive;
- token invalidation after rescan or successful selection;
- sanitized scan failures that do not expose upstream stderr;
- session-local selection, with no new persistence or production dependency.

Executable paths, bind addresses, ports and raw Windows endpoint IDs remain
outside WebView requests and responses. The UI does not own the worker or Route
lifecycle.

## Evidence

- `cargo xtask fmt`: passed, including frontend `vue-tsc`;
- `cargo xtask check`: passed;
- `cargo xtask test`: passed;
- `cargo xtask ci`: passed, including repository/document/manifests, Adapter
  smoke, frontend typecheck and production build;
- `cargo test -p capyio-desktop`: 22 passed, 2 physical tests ignored;
- real ignored `probes_real_user_supplied_audio_share_cli`: passed against the
  hash-verified v0.3.4 CLI and current Windows endpoint inventory.

The real probe proves enumeration and parser compatibility. The UI selection
path is compiled and contract-tested but was not counted as another audible
physical run; the prior `001A3` report retains the audible and lifecycle
evidence.

## Remaining Gate 7A work

- longer background and secure-lock behavior;
- audio-focus interruption and recovery;
- latency, jitter, quality and soak measurements;
- production pairing/encryption and distributable packaging;
- persistence/default-device policy and broader endpoint hot-plug testing.
