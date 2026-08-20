# HardwarePool Roadmap

The roadmap uses gates instead of calendar promises. A gate closes only when its acceptance evidence exists.

## Gate 0 — Repository bootstrap

Status: **complete**.

Deliverables:

- product requirements v0.2;
- architecture, protocol, audio, security and testing specifications;
- Agent instructions and task workflow;
- Cargo/pnpm monorepo skeleton;
- pure Rust Core, Runtime, Protocol and testkit;
- CLI demo;
- Vue/Tauri UI with Mock Backend;
- CI workflow drafts.

Exit criteria:

- first online dependency install generates lockfiles;
- `cargo xtask ci` passes locally;
- frontend typecheck/build passes;
- CI passes on Windows, Linux and macOS.

Windows-local evidence covers lockfile generation, the unified CI command, all
deterministic tests and demos, Browser Mock, and the Tauri desktop build/runtime.
Commit `4a658d5` additionally passed hosted Windows/Linux/macOS Rust CI, Linux UI
build and Linux repository validation on 2026-08-21.

## Gate 1 — Core contract hardening

- property/state-transition tests;
- stable error codes;
- Profile validation registry;
- operation completion/cancellation seam: **complete** (`HP-CORE-003`,
  commit `d1dd746`);
- golden Protobuf fixtures;
- structured diagnostics format;
- architecture dependency checks.

## Gate 2 — Android Audio Lab

- generated Tauri Android project;
- Kotlin foreground service and permission plugin;
- local microphone capture to WAV;
- local speaker tone/WAV render;
- actual parameter/xrun display;
- route/focus/background test evidence;
- Adapter selection ADR.

## Gate 3 — Application-level reference audio

- reference control transport in local LAN;
- reference UDP/PCM or equivalent test data transport;
- Windows test source → Android speaker;
- Android microphone → Windows WAV;
- bounded jitter buffer;
- initial drift estimator/resampler;
- network impairment proxy and metrics.

No Windows driver is required for this gate.

## Gate 4 — Production transport decision

- compare reference transport, AOO Adapter and at least one alternative;
- measure latency, loss behavior, CPU, battery and implementation risk;
- decide control-channel authentication and data-channel encryption;
- record decision in ADR;
- implement downgrade/replay protections before untrusted-network use.

## Gate 5 — Windows Driver Spike

- provision isolated Windows test environment;
- build and install unmodified Microsoft sample baseline separately;
- document endpoint topology and legal provenance;
- define versioned Broker-driver IPC;
- expose fixed virtual speaker and microphone;
- test endpoint lifecycle without networking.

## Gate 6 — Windows system-level speaker

- Windows Audio Engine → virtual endpoint → Broker → Android speaker;
- endpoint format conversion and clock recovery in user mode;
- offline/disconnect behavior;
- two-hour soak and latency evidence.

## Gate 7 — Windows system-level microphone

- Android microphone → Broker → virtual capture endpoint;
- silence behavior while offline;
- permission revocation;
- application compatibility matrix;
- two-hour soak.

## Gate 8 — Full duplex and release hardening

- independent and combined operation;
- AEC/NS/AGC negotiation;
- feedback-safe UI;
- sleep, lock, route, network and process-crash recovery;
- Driver Verifier, static analysis and selected HLK;
- signed installer planning, SBOM and security review.

## Gate 9 — First public pre-release

- reproducible build documentation;
- signed artifacts where required;
- support matrix backed by evidence;
- issue templates and diagnostics bundle;
- migration and protocol compatibility policy.

## Post-MVP profiles

Only after the audio MVP is stable:

- camera capture and virtual camera;
- display source/sink and virtual monitor;
- keyboard, pointer and touch/HID;
- IMU and generic sensors;
- actuators;
- remote inference/compute Profile;
- automatic discovery;
- additional system projections for Linux and macOS;
- multiple nodes and WAN relay.
