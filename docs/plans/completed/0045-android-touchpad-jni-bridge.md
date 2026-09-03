# CAPY-PTP-002U — Android MotionEvent JNI bridge

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `NFR-STAB-002`, `NFR-SEC-003`,
`NFR-PERF-001..003`, `NFR-MAINT-001..003`

## Objective

Build and verify the first real Android framework-to-Rust boundary for complete
touchpad contacts, while retaining private packet and platform boundaries.

## In scope

- primitive-only JNI contract version 1;
- creating-thread-owned native session registry;
- `MotionEvent` action, pointer identity, tool, coordinate and pressure mapping;
- explicit start/stop/close cancellation packets;
- host composition tests for one- and two-finger lifecycle;
- real `arm64-v8a` native build and release AAR packaging;
- Android wireless-debug connection and read-only ABI/API verification.

## Out of scope

- APK, Activity, UI, service or Android permission;
- installing anything on the physical phone;
- socket, pairing, authentication, encryption or discovery implementation;
- Windows input injection in the Android build step.

## Acceptance criteria

1. Android framework objects do not cross JNI.
2. Every pointer array is length-consistent and bounded by the existing contract.
3. Initial and terminal cancellation retain contiguous private sequences.
4. Panics cannot unwind across JNI and sessions cannot cross Kotlin threads.
5. Host tests, ARM64 build and release AAR assembly pass.
6. The AAR contains the ARM64 JNI library and declares no permission/component.

Detailed evidence: `docs/CAPY_PTP_002U_REPORT.md`.
