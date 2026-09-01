# CAPY-PTP-002V — Live Android-to-Windows touchpad lab

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`, `NFR-STAB-001..004`,
`NFR-SEC-001..003`, `NFR-PERF-001..003`, `NFR-MAINT-001..003`

## Objective

Prove one real closed Android `MotionEvent` gesture reaches and is accepted by a
real Windows virtual Precision Touchpad through bounded private records.

## In scope

- explicitly authorized `INTERNET` permission and debug APK installation;
- landscape 1–5 finger Android lab touch surface;
- bounded sender queue and exact per-frame Ack;
- ADB-paired reverse tunnel with loopback-only Windows listener;
- full Hello binding validation before real virtual device creation;
- one automated Android single-finger swipe and native Windows cleanup;
- exact installed-APK hash and permission evidence.

## Out of scope

- production peer authentication, direct LAN/WAN transport or discovery;
- foreground/background service and reconnect;
- production signing or distribution;
- Windows driver installation or system security-policy changes;
- human observation claims for one-, three- or four-finger Windows actions.

## Acceptance criteria

1. APK contains only the explicitly approved Internet permission.
2. Installed APK bytes equal the locally built, inspected APK.
3. Listener is loopback-only and requires both desktop-input gates.
4. Hello binds the exact route before virtual device creation.
5. Android Data records are acknowledged only after real native submission.
6. A closed Android gesture processes at least cancel/down/move/release and
   closes the virtual device without active contacts.
7. Reverse mapping and temporary device file are removed after the run.

Detailed evidence: `docs/CAPY_PTP_002V_REPORT.md`.
