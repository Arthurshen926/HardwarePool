# CAPY-CAMERA-001B1C Report

Date: 2026-08-29

Status: implementation, Frame Server roundtrip and exact rollback complete;
changes uncommitted.

Base: CAPY-IO-CONTRACTS-001 at fc3da36

Branch: codex/capyio-camera

## Outcome

The Windows Media Foundation boundary now provides a complete deterministic
virtual-camera fixture path:

- standard `DllGetClassObject` and `DllCanUnloadNow` exports;
- a fixed class factory that returns `IMFActivate`, with activation attributes
  copied into an `IMFMediaSourceEx` source;
- mandatory `IMFGetService`, `IKsControl` and `IMFSampleAllocatorControl`
  behavior plus a mandatory Legacy sensor profile;
- one `IMFMediaStream2` with NV12 1280x720@30 metadata and deterministic
  samples allocated from the Frame Server-provided allocator;
- a closed session/current-user `SoftwareCameraSource` registrar backend;
- a closed `preflight`/`roundtrip`/`cleanup` lab program and a hash-pinned,
  fixed-target administrator deploy/remove script.

The source initializes a ten-sample provided allocator pool for the selected
media type. `RequestSample` uses non-blocking locks, allocates from that pool,
writes through `IMF2DBuffer2::Lock2DSize`, sets current length and timing,
preserves an optional token and queues `MEMediaSample`. A generation failure
cancels its accepted request ticket and publishes `MEError`.

## Automated evidence

The Windows tests cover:

- factory activation, interface querying, aggregation rejection, server locks
  and unload tracking;
- activation-attribute propagation and the Legacy sensor profile collection;
- allocator negotiation and provided-allocator sample delivery;
- exact media type, source/stream event order, token identity, two NV12 frames,
  exact 333333-tick local timing, pause/resume, stop/restart and shutdown;
- inert backend construction, the closed session/current-user plan and
  case-insensitive Windows symbolic-link matching.

`cargo test -p capyio-windows-camera-mf --all-targets` passes all five tests.
The final `cargo xtask ci` workspace gate also passes, including formatting,
check, Clippy, Rust tests, documentation/manifests, Adapter Smoke and desktop
typecheck/build.

## Controlled system evidence

The final release DLL is 174080 bytes with SHA-256
`FA1389606818D4753F064B66F1B484C73D059E8E91DE36EF11258EDDCAC355BE`.
The exact package, ACL and CLSID values were verified after deployment.

On `DESKTOP-AT8EVE9`, the elevated closed roundtrip passed registration, exact
enumeration, `GetMediaSource` and two-frame Source Reader delivery. It validated
the full NV12 byte count, exact sample duration, monotonic bounded downstream
timestamps and a BT.709-limited Y value copied to CPU memory. The command then
stopped and shut down the virtual camera.

The final cleanup/preflight reported no session registration. Administrator
rollback removed the exact CLSID and DLL on the first attempt for the final
artifact. Final verification found no DLL, CLSID or CapyIO ProgramData
directory. See `CAPY_CAMERA_001B1C_LAB_REPORT.md` for host-specific evidence.

## Important host behavior

The same start operation returned `0x80070005` without elevation on this
Windows build even though all relevant camera privacy toggles were already on.
The system check therefore establishes an elevated lab path, not a supported
ordinary-user product workflow. No privacy or security setting was changed.

## Deferred work

- perform a separately bounded ordinary-camera-application compatibility check;
- decide how the product should handle this host's elevation requirement;
- integrate a real camera producer only through a later reviewed Adapter and
  high-bandwidth data-plane slice;
- keep Android capture, codecs and network transport out of this completed
  fixture projection gate.
