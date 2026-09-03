# CAPY-CAMERA-001C27 — registered producer stall recovery

Date: 2026-08-31

Status: implementation, exact release packaging, focused Windows tests and
warnings-denied Clippy complete. Full repository CI and read-only clean-host
preflight are recorded below. No COM DLL deployment, virtual-camera
registration, APK operation, ADB operation or physical camera run occurred in
this slice.

## Trigger

CAPY-CAMERA-001C26 allowed a registered virtual camera to attach when the
producer appeared after activation, but its handoff was one-way. Once a shared
frame had arrived, an indefinitely stalled producer caused the retained Media
Foundation request to retry forever. A consumer handle also kept the old named
mapping alive after its producer exited, preventing a replacement producer from
owning the fixed name until the virtual camera closed.

## Outcome

- Every registered activation now uses the same asynchronous lifecycle
  provider, whether the fixed production mapping is present or absent at
  activation. Direct caller-owned in-process shared-ingress constructors remain
  unchanged.
- A live mapping may be empty for 400 consecutive 5 ms sample-pump polls. This
  is a nominal two-second stall window, not a hard wall-clock guarantee.
- At the bound, the provider releases its read-only mapping handle and resumes
  the deterministic 720p30 placeholder at the next virtual output sequence and
  timestamp. The first fallback frame is marked discontinuous.
- Placeholder mode checks only `Global\CapyIO.CameraIngress.v1` after the
  existing fixed 15-placeholder-frame countdown. A fresh validated publication
  is rebased to the same virtual stream/epoch/timeline and marked discontinuous.
- The provider records producer generation, source identity, source sequence
  and source timestamp before rebasing. Reopening a paused producer cannot
  replay its last publication as a new frame; that read handle is released and
  placeholder output continues until a newer publication exists.
- `DeterministicNv12Source::new_at_sequence` can resume fixture rendering at an
  explicit sequence/timestamp while preserving checked rational frame
  durations and overflow handling.
- Access denial, malformed shared state, non-monotonic live data and every
  mapping error other than exact file-not-found continue to fail closed.

No shared-memory ABI, mapping name, registration scope, permission, transport,
codec or third-party dependency changed.

## Automated evidence

- `cargo test -p capyio-windows-camera -p capyio-windows-camera-mf` — PASS.
  The focused matrix includes eight fixture tests and nine MF library tests.
- New regressions prove:
  - fixture output resumes at an existing non-zero sequence/timestamp;
  - a stopped producer yields `MF_E_NOTACCEPTING` only inside the fixed stall
    window, then one continuous discontinuous placeholder frame;
  - releasing the old producer and consumer permits a replacement producer to
    own the same mapping and reattach without reopening the media source;
  - reopening a paused producer does not replay its stale last publication;
  - a later fresh publication from that same producer resumes live output.
- `cargo clippy -p capyio-windows-camera -p capyio-windows-camera-mf
  --all-targets -- -D warnings` — PASS.
- Repository structural validation pins the two fixed bounds, unified
  registered provider, resumable fixture, fallback/reattach paths, stale-frame
  guard and exact release hashes.
- `cargo xtask ci` — PASS, including workspace format/check/Clippy/tests,
  documentation/manifests, Adapter smoke, repository validation and desktop
  typecheck/build.
- `scripts/capyio-camera-live-lab-preflight.ps1` — PASS against the C27
  hash-locked artifacts and clean host state without elevation or mutation.

## Exact release artifacts

- Receiver, 249,856 bytes:
  `E1541F739572294D14FB740290A7AF09E334D0D05A888FE8C62805C4E0C81833`.
- Virtual-camera lab, 323,072 bytes:
  `F87DD851358A2FAEFDFC2BC0A083CD5461BD48C17994ADA53DA8ECD5F3B7EFF5`.
- COM DLL, 194,048 bytes:
  `4C236858C5223B4A1303E825496EBE6799C52E9EAE366DC6DE41C8E9A88F70F0`.

The parameter-free read-only preflight and administrator deployment script pin
these exact artifacts. Preparing and hashing them made no system change.

## Remaining evidence and limits

A separately authorized system regression is still required to prove all three
ordinary Windows application transitions with the exact package:

1. placeholder to late phone stream;
2. live phone stream to placeholder after producer loss;
3. placeholder back to a restarted phone stream without reopening Windows
   Camera.

The stall counter advances only while Frame Server has a pending sample request;
it is intentionally not a producer heartbeat. Another independent consumer can
still keep an abandoned mapping alive and prevent a replacement producer from
creating the same fixed name; C27 releases only this provider's own handle.
Persistent registration, service ownership, an offline status overlay,
pairing/encryption, background Android capture and broad device compatibility
remain outside this slice.
