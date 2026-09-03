# CAPY-CAMERA-001B1E Report

Date: 2026-08-29

Status: bounded process-local decoded-frame ingress and MF projection complete;
cross-process ingress not started; changes uncommitted.

Base: CAPY-IO-CONTRACTS-001 at fc3da36

Branch: codex/capyio-camera

## Outcome

`ExternalNv12FrameIngress` is now the first concrete seam between a future
decoder/transport worker and the Windows virtual-camera projection. It accepts
only owned frames matching the canonical 1280x720 30 fps packed NV12 contract
and binds them to one typed stream ID and positive epoch. It requires strictly
advancing sequence and source timestamps and rejects malformed payloads,
identity/epoch mismatches and end-of-stream.

The queue capacity is fixed at construction inside the existing 1..=12 frame
bound. Overflow drops the oldest frame, increments bounded metrics and marks the
next retained frame discontinuous. No codec, network protocol, Windows mapping
or device access was added.

`capyio-windows-camera-mf` now has a non-registered constructor that consumes
this ingress. Frame Server-facing `RequestSample` uses `try_lock` for both its
runtime and ingress. Empty or contended ingress returns
`MF_E_NOTACCEPTING`; it does not wait. Accepted frame bytes are copied through
the existing provided allocator and QPC timing mapper. A queue gap is projected
as `MFSampleExtension_Discontinuity`.

The registered COM class factory intentionally continues to construct the
deterministic fixture provider. Moving decoded frames from the background
Runtime into the Frame Server process requires a separate, versioned and
security-reviewed process-boundary slice.

## Automated evidence

The new pure tests prove:

- fixed capacity and drop-oldest behavior;
- discontinuity marking after overflow;
- exact stream/epoch binding;
- transactional rejection of duplicate/regressing sequence and timestamps;
- rejection of invalid capacity, payload length and end-of-stream.

The Windows in-process COM test queues three distinct caller-owned frames into a
two-frame ingress. It observes only the two retained luma values in allocated MF
samples, verifies the first sample's discontinuity attribute, confirms exact
333333-tick spacing and receives `MF_E_NOTACCEPTING` once the ingress is empty.

Targeted Camera tests and Clippy pass. The final workspace-wide `cargo xtask ci`
also passes, including formatting, check, Clippy, all Rust tests and doctests,
the deterministic demo, documentation/manifests, Adapter Smoke and desktop
typecheck/build.

## Controlled system regression

The common `RequestSample` implementation changed, so the existing
fixture-backed system roundtrip was repeated on `DESKTOP-AT8EVE9` with the new
release DLL. The DLL is 175616 bytes with SHA-256
`14BC961B36A7AD40116C1AEFA46F38DA2F3EAA190940CF6330C4C62215CD0948`.
The fixed deployment hash, CLSID path and `ThreadingModel=Both` were verified;
the elevated `roundtrip` returned exit code 0.

An earlier pre-final rebuild with SHA-256
`2EAFA5170B3138757A8EECE238B6B5918FEFB5359A436A5A76E7A245053C4421`
also passed roundtrip and was rolled back; it is not used as final artifact
evidence. After the final hash above passed, one hidden UAC cleanup invocation
did not execute. Read-only verification therefore still found the complete
fixed registration and exact authorized DLL rather than assuming success. A
visible fixed-target UAC removal then returned 0. Final checks found no DLL,
CLSID or `C:\ProgramData\CapyIO` lab directory, and non-registering preflight
reported `existing_registration=false`.

This regression proves that fixture-backed system activation did not regress.
It does not prove external-frame delivery across the Frame Server process
boundary.

## Next gate

The next Windows slice should define a versioned, bounded shared-memory ingress
owned by the background Runtime, including generation, producer identity, ACL,
layout validation, newest-frame policy and crash cleanup. Only after that seam
passes can an Android/transport Adapter feed the registered virtual camera for
the first phone-to-PC experiment.
