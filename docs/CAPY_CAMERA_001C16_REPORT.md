# CAPY-CAMERA-001C16 — Continuous virtual camera across Android lens switch

Date: 2026-08-30

Status: implementation and hardware-free validation complete; controlled
front/back regression remains pending approval of the rebuilt hashes.

## Trigger evidence

The controlled C14 run successfully enumerated `CapyIO Camera` and Windows
inbox Camera visibly displayed live V2419A pixels at 1280x720. The receiver
decoded and published 816 frames with distinct first/last checksums. When the
user switched Android from the rear to the front lens, the existing C11 design
correctly closed Camera2, MediaCodec and the CAVC socket so that the new source
would receive a new stream identity and epoch. The original single-connection
lab receiver treated that clean EOF as completion; `live-hold` then failed
closed and removed the temporary virtual camera. Final rollback and the C15
clean-host preflight passed.

## Implementation

- Normal receiver behavior is unchanged unless
  `--reconnect-grace-millis` is explicitly supplied.
- The option is bounded to 1..=10000 ms. `live-hold` supplies the fixed value
  5000 and still accepts no caller parameters.
- On an eligible EOF the receiver retains its existing shared publisher and
  Global mapping while accepting only a loopback peer during the grace window.
- Every reconnected CAVC session gets a fresh record guard and Media Foundation
  decoder. It must retain canonical 1280x720 NV12 at 30 fps.
- Frames from the new CAVC identity are projected onto the continuous
  Adapter-owned virtual-camera identity with monotonic publication sequence and
  timestamps. The first resumed publication is marked discontinuous.
- The existing 60-second parent supervision, child cleanup, registrar
  Stop/Shutdown and mapping-removal checks remain unchanged.

This is an `AdapterManaged` continuity contract. It does not claim that the two
Android CAVC stream identities are one protocol stream.

## Automated evidence

- Receiver tests cover exact decoded-frame projection, delayed loopback
  reconnect acceptance and monotonic timing after 816 prior publications.
- The virtual-camera lab tests pin the exact five-token child argument vector.
- Focused receiver and virtual-camera tests and warnings-denied Clippy pass.
- Repository structural validation, documentation validation, `git diff
  --check` and the full `cargo xtask ci` pass.
- Release artifacts prepared for controlled regression:
  - receiver SHA-256
    `C50C3F4E700CD692720E8D3E4B31B8FA2E7BBD5F6D1A81331D024040CA60068E`;
  - orchestration executable SHA-256
    `93F17EB3EE97BD81568F3A13F58B5285B67792A7B4E7AB501DD45B202B24263A`;
  - COM DLL SHA-256
    `9437E24EA1274DE68B339D1A5F94467CF41C6C713CDC2B1E425A6433B79D213C`.

## Remaining controlled evidence

After exact approval, repeat the fixed deployment on the verified V2419A,
start rear capture, open Windows inbox Camera, switch Android to front and back,
and require visible recovery without losing the enumerated camera. Retain the
two-or-more CAVC config records, continuous publication counters and final
clean-host preflight. No pixels need to be persisted.
