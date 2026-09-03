# CAPY-CAMERA-001B1D Report

Date: 2026-08-29

Status: bounded sequential cross-process validation complete; simultaneous
multi-consumer fan-out unresolved; changes uncommitted.

Base: CAPY-IO-CONTRACTS-001 at fc3da36

Branch: codex/capyio-camera

## Outcome

The closed Windows virtual-camera lab can now keep one session/current-user
registration alive while exactly two separately spawned consumer processes run
one after the other. Each consumer independently:

- initializes Media Foundation;
- enumerates the exact symbolic link observed by the parent;
- activates the device through `IMFActivate`;
- reads and validates two 1280x720 30 fps NV12 samples through Source Reader;
- shuts down its activated media source before exiting.

The parent resolves only its current executable, invokes the fixed
`consumer-probe` command without a shell, applies a 20-second deadline to each
child, propagates child failures and kills/reaps an unfinished child before
registrar cleanup. The symbolic link is carried only in a child environment
value and is rejected unless it is bounded, contains no control character and
has the Windows software virtual-camera prefix.

The media-source core also accepts a repeated `Start` while already active. It
retains the existing stream generation, pending requests and timeline state and
publishes updated-stream, stream-started and source-started events. The Windows
projection correspondingly retains the Frame Server allocator and deterministic
sample generator instead of reinitializing them.

## Automated evidence

Targeted format, check, test and Clippy gates pass for
`capyio-windows-camera` and `capyio-windows-camera-mf`. New coverage verifies:

- repeated active `Start` retains generation, requests and timeline state;
- the in-process COM source continues its exact 333333-tick timeline after a
  repeated active `Start`;
- the lab command surface rejects extra parameters;
- symbolic-link matching follows Windows case-insensitive semantics;
- the child accepts only a bounded software virtual-camera link shape;
- a parent preserves the negative HRESULT reported by a failed child.

The final workspace-wide `cargo xtask ci` passes, including formatting,
workspace check and Clippy, all Rust tests and doctests, the deterministic demo,
documentation/manifests, Adapter Smoke and desktop typecheck/build.

## Controlled host evidence

On `DESKTOP-AT8EVE9`, the final media-source DLL used for the 001B1D lab is
174080 bytes with SHA-256
`3DE60DA0F84ACBAFA424EB4E356342D0B8C9B2B81F738AD34CCA09892425CCD8`.
The elevated `shared-roundtrip` returned exit code 0: both sequential external
consumers independently enumerated, activated and validated two frames. The
original direct `roundtrip` was then repeated and also returned exit code 0.

The final source rebuild changed only the lab executable's human-readable
closed-command help text; the deployed DLL and sharing logic remained
unchanged. The current release lab executable is 291328 bytes with SHA-256
`8B7A921923375AE835BA198042D2CCD8BEBB48B3B28F02ABA6E88BDC3BEF21F0`.

Cleanup and non-registering preflight reported no remaining registration.
Administrator removal encountered one transient Frame Server DLL mapping;
after verifying the remaining file had the exact authorized hash and the CLSID
was already absent, an idempotent retry succeeded. Final read-only checks found
no DLL, fixed CLSID or CapyIO ProgramData directory. See
`CAPY_CAMERA_001B1D_LAB_REPORT.md` for the host-specific record.

## Unresolved concurrency result

An intentionally simultaneous two-consumer diagnostic consistently failed with
`MF_E_HW_MFT_FAILED_START_STREAMING` (`0xC00D3704`) on the recorded host. A
staggered diagnostic first exposed `MF_E_INVALID_STATE_TRANSITION`
(`0xC00D3E82`); correcting repeated active `Start` removed that local state
transition defect, but did not make simultaneous consumers succeed.

This gate therefore proves process-independent sequential reuse while one
parent owns the session camera. It does not prove simultaneous multi-app
fan-out, broad third-party application compatibility, a non-elevated production
lifecycle, physical-camera capture or remote frame ingress.
