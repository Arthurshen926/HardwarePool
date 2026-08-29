# CAPY-CAMERA-001 / CAPY-IO-CONTRACTS-001 — Parallel I/O foundation

Status: active

Owner: Codex

Created: 2026-08-29

Requirements: `FR-SCEN-003`, `FR-SCEN-004`, `FR-SCEN-006`, `FR-PORT-002..005`,
`NFR-STAB-002..004`, `NFR-MAINT-001..004`

## Objective

Establish one stable Video/Input semantic baseline from `main`, then begin a
hardware-free deterministic camera slice while the existing microphone work
continues independently.

## Slices

1. `CAPY-IO-CONTRACTS-001`: portable Video/Input crates, canonical Profile
   helpers, validation/tests and reserved non-functional workspace boundaries.
2. `CAPY-CAMERA-001A`: deterministic 720p30 NV12 color-bars/moving-clock
   producer and bounded frame queue; no phone or system registration.
3. `CAPY-CAMERA-001B`: Windows 11 user-mode Media Foundation virtual-camera
   feasibility boundary and generated-frame projection, after a dedicated ADR
   fixes lifecycle/registration/rollback and official SDK prerequisites.
4. `CAPY-CAMERA-000`: read-only Android Camera2 inventory only after the exact
   device, command path and privacy retention are approved; no camera capture.
5. `CAPY-CAMERA-002`: reviewed VCamdroid-compatible external Adapter. Its
   H.264/RTSP data plane remains private `AdapterManaged` traffic.

Parallel worktrees after the contracts baseline has an approved commit:

- `codex/capyio-camera` — Camera fixture/projection;
- `codex/capyio-gamepad` — recorded IMU to deterministic DSU mapping;
- `codex/capyio-android-node` — platform-neutral module registry first;
- `codex/capyio-touchpad` — specification/fixtures until a slot opens.

## Current safety boundary

- no driver/APK install or removal;
- no Android permission or foreground-service declaration change;
- no virtual-camera registration, driver deployment or system security change;
- no phone camera capture or personal video fixture;
- no upstream source import, FFmpeg, codec or new production dependency;
- no commit, push or pull request without explicit human approval.

## Acceptance for the common baseline

- canonical Profile helpers match `PORT_PROFILES.md`;
- Video/Input validation covers bounds, exact negotiation, epoch/sequence gaps,
  stale/future data, fail-safe resets and neutral state;
- workspace marker crates compile while clearly reporting no implementation;
- full available Rust/document/manifest/Adapter checks pass;
- the microphone working tree remains unchanged.

## Progress

The common baseline implementation and available validation are complete. See
`docs/CAPY_IO_CONTRACTS_001_REPORT.md`. The baseline is intentionally
uncommitted pending explicit human approval; downstream worktrees and Camera
001A start from that approved commit.
