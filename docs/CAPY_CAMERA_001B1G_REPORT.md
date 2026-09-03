# CAPY-CAMERA-001B1G Report

- Date: 2026-08-29
- Branch: `codex/capyio-camera`
- Baseline: `fc3da3636ca6c969667e71a9b596dcc944380146`
- Scope: non-registering cross-process shared-frame to Media Foundation sample
- System mutation: none

## Outcome

B1G proves that the B1F shared-memory contract and the Media Foundation sample
path compose across an actual process boundary.

The parent test process creates a unique local mapping, validates and publishes
one canonical 1280x720 NV12 frame, and sets its first luma byte to 137. It then
directly spawns the current Rust test executable with one exact child-test name.
The child:

1. opens the existing mapping through `CameraSharedIngressConsumer`, which
   requests `FILE_MAP_READ` only;
2. starts COM/Media Foundation through `MediaFoundationRuntime`;
3. constructs the non-registered shared-ingress media source;
4. supplies an `IMFVideoSampleAllocator`;
5. starts the source and requests one sample;
6. locks the resulting 2D NV12 buffer and observes luma byte 137;
7. verifies the canonical 333,333 100-ns-unit sample duration;
8. verifies a second request fails immediately with `MF_E_NOTACCEPTING`;
9. stops and shuts down the source.

This advances the evidence from “another process can read the raw mapped
frame” to “another process can project that frame as a platform-allocated MF
sample.” Registered COM activation remains fixture-backed.

## Boundary decision

The generic `capyio-runtime` is OS-independent and must not depend on Win32 file
mapping. The existing `capyio-windows-service`/`CapyIOBroker` is explicitly the
virtual-speaker Broker and should not silently become a combined camera host.
The production mapping module should therefore be extracted into a dedicated
Windows camera-share crate that can be consumed by both the MF DLL and a
separate background camera host. B1G records this dependency direction but does
not perform that structural extraction.

## Automated evidence

Targeted shared-ingress tests passed:

```text
cargo test -p capyio-windows-camera-mf --lib shared_ingress::tests -- --nocapture
```

The full target suite and strict linting also passed:

```text
cargo clippy -p capyio-windows-camera-mf --all-targets -- -D warnings
cargo test -p capyio-windows-camera-mf --all-targets
```

The final repository gate also passed:

```text
cargo xtask ci
```

## Safety and rollback

The test uses a unique bounded `Local\\CapyIO.CameraIngress.v1.test...` name
provided only to a directly spawned copy of the current test executable. It
does not invoke a shell or accept an external executable. No global production
mapping, registry key, COM deployment, virtual-camera registration, driver,
service, APK, boot option or security policy is changed. The local mapping is
destroyed when the final process handle closes, so no rollback command is
required.

## Remaining work

- extract the shared mapping contract from the COM crate into a dedicated
  Windows camera-share crate;
- add a separate background camera host that owns producer lifecycle;
- verify the production global mapping under the intended service identities;
- switch registered activation to the shared consumer only after those checks;
- run a controlled system external-frame roundtrip and exact rollback;
- add Android capture, transport/decode and Route authorization in later gates.
