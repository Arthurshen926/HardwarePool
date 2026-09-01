# CAPY-CAMERA-001B1F Report

- Date: 2026-08-29
- Branch: `codex/capyio-camera`
- Baseline: `fc3da3636ca6c969667e71a9b596dcc944380146`
- Scope: versioned Windows decoded-frame shared memory and non-registered MF
  provider
- System mutation: none

## Outcome

B1F implements the first explicit process boundary between a future Runtime
frame producer and the Media Foundation source process. It does not yet select
that provider from the registered COM class factory.

One producer creates `Global\\CapyIO.CameraIngress.v1`; consumers request a
read-only view. The fixed v1 mapping is 4,147,648 bytes:

- one 256-byte, 64-byte-aligned header;
- three 64-byte, 64-byte-aligned slot headers;
- three fixed 1,382,400-byte packed NV12 payloads.

The header binds magic, ABI version, all sizes, canonical 1280x720 30 fps
NV12/BT.709 metadata, producer process ID, stream ID, positive epoch and a
non-zero generation. A second producer for the same object fails with
`AlreadyOwned`. Publication uses a monotonic global number plus a per-slot
commit number. A consumer checks the commit before and after its bounded owned
copy and advances only its private cursor. It returns no frame when the newest
slot is empty or being replaced, and marks a frame discontinuous when that
reader skipped one or more publications.

The production SDDL is fixed to:

```text
D:P(A;;GA;;;SY)(A;;GR;;;LS)(A;;GA;;;BA)(A;;GA;;;OW)
```

SYSTEM, administrators and the mapping owner receive full access; LocalService
receives generic read only. The generation is stale-instance evidence, not an
authentication secret. A host running as LocalService has not yet exercised
the effective ACL, so that remains a later controlled boundary test.

`create_in_process_media_source_with_shared_ingress` connects an already-open
consumer to the existing Media Foundation provider abstraction. It returns
`MF_E_NOTACCEPTING` when there is no new stable publication and does not wait.
The registered class factory and all existing system-camera lab commands remain
fixture-backed.

## Changed files

- `platform/windows/capyio-camera-mf/src/shared_ingress.rs`
  - fixed mapping ABI, protected creation, single-producer ownership;
  - read-only latest-frame consumers and per-reader discontinuity;
  - true child-process mapping test.
- `platform/windows/capyio-camera-mf/src/windows_impl.rs`
  - non-registered shared provider and non-blocking empty behavior.
- `platform/windows/capyio-camera-mf/src/lib.rs`
  - exports the bounded producer/consumer contract and constructor.
- `platform/windows/capyio-camera-mf/Cargo.toml`
  - uses workspace `capyio-video` and the already-resolved target-specific
    `windows-sys` 0.61.2 binding.
- architecture, security, testing, active plan, ADR and third-party records.

The subsequent B1H slice moved `shared_ingress.rs` without changing its v1 ABI
into `platform/windows/capyio-camera-share/src/lib.rs` and left MF dependent on
the read contract.

## Automated evidence

The targeted Windows suite passed:

```text
cargo test -p capyio-windows-camera-mf --all-targets
```

Relevant B1F cases:

- fixed layout, SDDL and duplicate-owner rejection;
- two independent consumers read the same latest stable frame;
- skipped publication sets discontinuity;
- a separately spawned test executable opens the mapping read-only and verifies
  sequence plus payload byte 77;
- the shared MF provider returns payload byte 91 and fails fast with
  `MF_E_NOTACCEPTING` after consumption.

Strict target linting also passed:

```text
cargo clippy -p capyio-windows-camera-mf --all-targets -- -D warnings
```

The final repository validation also passed:

```text
cargo xtask ci
```

## Safety and rollback

Normal tests use unique `Local\\CapyIO.CameraIngress.v1.test...` mapping names.
Mappings have no filesystem/registry persistence and disappear when their final
handle closes. This slice did not deploy a DLL, register/start/remove a virtual
camera, change a Windows service, install a driver/APK or alter boot/security
policy. No rollback command was required.

## Remaining risks and next gate

- The Runtime/Service does not yet own and publish the production global
  mapping.
- Registered COM activation still constructs the deterministic fixture.
- The LocalService read ACE is specified and unit-pinned but not yet verified
  under the actual Frame Server service token.
- The latest-frame triple buffer is intentionally lossy; it is not a recorder
  queue and a slow reader observes a discontinuity.
- Android Camera2 capture, encoded transport, decode and peer authorization are
  absent.
- Simultaneous ordinary-application fan-out remains unresolved from B1D.

The next safe slice is to add a bounded Runtime-side producer owner and a
controlled non-registering producer-to-MF integration process. Switching the
registered class factory and repeating a system roundtrip should remain a
separate explicitly reviewed gate.
