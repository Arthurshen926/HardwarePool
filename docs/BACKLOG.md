# HardwarePool Bootstrap Backlog

This backlog turns the roadmap into small Agent-sized tasks. Each task must be copied into an issue or `docs/plans/active/` plan before implementation. Requirement IDs are authoritative; do not implement later tasks merely because they appear here.

## P0 — Validate and stabilize the archive

### HP-BOOT-001 — First online dependency resolution

**Requirements:** `NFR-MAINT-001`, `NFR-MAINT-002`  
**Goal:** establish reproducible dependency locks without changing architecture.

Acceptance criteria:

- install the pinned Rust toolchain and pnpm;
- run `cargo xtask doctor`;
- resolve dependencies successfully;
- generate and commit `Cargo.lock` and `pnpm-lock.yaml`;
- record any unavailable version and the chosen replacement in an ADR;
- do not start platform implementation in this task.

Required evidence:

```text
cargo --version
rustc --version
node --version
pnpm --version
cargo metadata --format-version 1
pnpm install
```

### HP-BOOT-002 — Compile and repair the bootstrap workspace

**Requirements:** Gate 0, `NFR-MAINT-001..004`  
**Depends on:** HP-BOOT-001.

Acceptance criteria:

- `cargo fmt --all -- --check` passes;
- `cargo check --workspace --exclude hardwarepool-gui --all-targets` passes;
- Clippy passes with warnings denied;
- all non-Tauri Rust tests pass;
- fixes remain inside the documented architecture boundaries.

### HP-BOOT-003 — Build the shared web UI

**Requirements:** `FR-UI-001..005`  
**Depends on:** HP-BOOT-001.

Acceptance criteria:

- `pnpm typecheck` passes;
- `pnpm build` produces `apps/gui/dist`;
- browser Mock mode displays two independent capabilities;
- toggling one capability does not change the other;
- UI still labels all metrics as simulated.

### HP-BOOT-004 — Build the desktop Tauri demo

**Requirements:** Gate C  
**Depends on:** HP-BOOT-002 and HP-BOOT-003.

Acceptance criteria:

- `pnpm tauri dev` starts on Windows;
- the UI uses `tauri_demo` rather than `browser_mock`;
- UI commands operate on the Rust `DemoLab`;
- no driver, microphone, speaker or network access occurs.

## P1 — Core contract hardening

### HP-CORE-001 — Property-test binding transitions

**Requirements:** `FR-SESSION-001..006`, `NFR-STAB-001`.

Acceptance criteria:

- every allowed transition has a test;
- every disallowed transition returns a stable typed error;
- random command sequences never create two live bindings for one capability;
- stopping microphone never changes speaker state and vice versa.

Adding a property-testing dependency requires a dependency ADR.

### HP-CORE-002 — Stable error-code registry

**Requirements:** `FR-PROTO-005`, `FR-DIAG-004`.

Acceptance criteria:

- public failures have stable machine-readable codes and human details;
- internal error strings are not used as protocol compatibility contracts;
- protocol conversion has golden tests.

### HP-CORE-003 — Operation completion and cancellation seam

**Requirements:** `NFR-STAB-002`, architecture section 9.

Goal: let asynchronous platform work complete back into the deterministic Runtime without allowing arbitrary callback threads to mutate Core state.

Acceptance criteria:

- opaque operation IDs;
- explicit pending/completed/cancelled/disposed states;
- bounded retained completions;
- deterministic tests for completion/cancellation races;
- no async runtime dependency in `hardwarepool-core`.

### HP-PROTO-001 — Golden v1 fixtures

**Requirements:** `FR-PROTO-001..006`.

Acceptance criteria:

- checked-in binary and debug-JSON fixtures for Hello, capability list and projection lifecycle;
- fixtures decode to expected Core values;
- field numbers and enum values cannot change unnoticed;
- unknown major and invalid IDs remain explicit failures.

## P2 — Platform laboratories

### HP-ANDROID-001 — Generate Tauri Android shell

**Requirements:** `FR-AND-001`, `FR-UI-005`.

Acceptance criteria:

- run the pinned Tauri Android initialization flow;
- record generated Gradle/SDK/NDK/JDK versions in `docs/TOOLCHAIN.md`;
- shared UI launches on the vivo test phone;
- no audio permission is requested yet;
- generated files are reviewed before committing.

Changing Android permissions is out of scope.

### HP-ANDROID-002 — Local speaker Audio Lab

**Requirements:** `FR-AND-002`, `FR-AUD-001`.

Acceptance criteria:

- render a generated tone and a bundled WAV locally;
- report requested and actual sample rate, channel count and burst/buffer values;
- expose start/stop through a platform Adapter, not directly through UI code;
- retain xrun and route-change diagnostics;
- no network dependency.

### HP-ANDROID-003 — Local microphone Audio Lab

**Requirements:** `FR-AND-003..005`, `NFR-SEC-001`.

This task requires explicit approval before changing permissions or installing the APK.

Acceptance criteria:

- visible microphone permission flow;
- foreground service with persistent indicator while capture is active;
- record PCM/WAV locally;
- stop immediately on revoke/service stop;
- test lock screen, background and focus contention;
- no network dependency.

### HP-WIN-USER-001 — Windows WASAPI Audio Lab

**Requirements:** preparatory work for `FR-WIN-001..006`.

Acceptance criteria:

- enumerate render/capture endpoints;
- play tone/WAV to a selected endpoint;
- record from a selected endpoint;
- optional shared-mode loopback capture tool;
- structured logs and no driver dependency.

## P3 — Reference distributed audio

### HP-TRANSPORT-001 — Reference control channel

**Requirements:** `FR-NODE-001..004`, `FR-PROTO-001..006`.

Acceptance criteria:

- manual-IP LAN connection;
- bounded length-prefixed Protobuf envelopes;
- connection state and errors feed the deterministic Runtime;
- no automatic discovery, cloud relay or production-security claim;
- malformed-frame integration tests.

### HP-TRANSPORT-002 — Reference PCM frame encoding

**Requirements:** `FR-AUD-003..007`, `NFR-PERF-001..003`.

Acceptance criteria:

- versioned binary frame header maps to `hardwarepool-audio::AudioFrame`;
- explicit stream ID, epoch, sequence, monotonic timestamp, sample index and payload length;
- bounded parser and fuzz/property tests;
- no silent reinterpretation of unknown version or sample format.

### HP-E2E-001 — Windows test source to Android speaker

**Depends on:** HP-ANDROID-002, HP-WIN-USER-001, HP-TRANSPORT-001/002.

Acceptance criteria:

- application-level path only; no virtual driver;
- 48 kHz stereo PCM baseline;
- loss/reorder/buffer/drift metrics;
- disconnect does not replay stale samples;
- retained WAV and latency evidence.

### HP-E2E-002 — Android microphone to Windows WAV

**Depends on:** HP-ANDROID-003 and the same transport tasks.

Acceptance criteria:

- application-level path only;
- 48 kHz mono PCM baseline;
- visible permission and foreground-service evidence;
- loss/reorder/buffer/drift metrics;
- revocation stops capture and closes the stream epoch.

## P4 — Windows driver spike

Do not begin these tasks on the daily-development Windows installation.

### HP-WIN-DRV-001 — Isolated driver lab

- identify exact Windows edition/architecture;
- create VM or dedicated test installation;
- configure checkpoints and WinDbg;
- document recovery procedure;
- no HardwarePool driver code yet.

### HP-WIN-DRV-002 — Unmodified sample baseline

- obtain the official sample from its authoritative source;
- preserve provenance and license notes;
- build and install it only in the test target;
- enumerate, stream, disable, enable, reboot and uninstall;
- retain exact build/deployment evidence.

### HP-WIN-DRV-003 — Fixed endpoint/IPC spike

- fixed `HardwarePool Speaker` and `HardwarePool Microphone` endpoints;
- thin PCM IPC only;
- Broker absent/crash behavior is safe;
- no network, codec, Protobuf, pairing or JSON in kernel code;
- Driver Verifier only after explicit approval.

## Task selection rule

Always choose the earliest unblocked task that produces measurable evidence. A later task must not be used to bypass a failing earlier gate.
