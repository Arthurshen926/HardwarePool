# ADR 1422: Integrate the completed display prototype with the unified Node

Status: accepted for source integration; product qualification pending

Date: 2026-09-17

## Context

The owner now includes Display in the unified-product scope. The display
worktree is based on `fc3da363` and its completed implementation is uncommitted.
A branch merge alone would omit it. Its final `CAPY_DISPLAY_003X_REPORT.md`
records controlled-lab media and topology restoration, not a consumer release.
The unified branch already contains newer audio, camera and input composition.

## Decision

Import the reviewed display sources into the integration branch without changing
the source worktree. Preserve existing Core, Runtime, protocol and audio/input
semantics when merging overlapping files. Display remains a separate typed
Profile and Adapter-owned media path. Integrate the Windows worker with the
existing service and the Android receiver with the existing APK, retaining one
shared Vue UI and Node identity.

The first slice adds the seven display Rust crates and their tests. It does not
enable a display worker or claim an Android receiver in the unified APK yet.
Subsequent slices must compose host lifecycle, shared controls and packaging.

The inherited source uses rustls 0.23.43 (Apache-2.0 OR ISC OR MIT), with its ring
provider, for user-mode TLS 1.3; rcgen 0.14.5 (MIT OR Apache-2.0) is test-only.
The Windows SDK bindings are windows 0.61.3 and windows-future 0.2.1
(MIT OR Apache-2.0). SHA-256 uses sha2 0.10.9 (MIT OR Apache-2.0).
These are inherited pinned-display dependencies, not new protocol designs.
Alternatives are bespoke crypto, dropping existing authenticated transport, or
rewriting platform bindings; all add avoidable risk. The source worktree's
dependency and GPL distribution records remain authoritative for its optional
Vibepollo/Moonlight native engine; no upstream binary or private key is imported
by this first source slice.

## Delivery gates

- One Android APK and Windows application expose every integrated capability.
- First-run connection, permission errors and unavailable dependencies have
  actionable UI; no terminal or independent lab launcher is required.
- Routes have independent Stop, failure cleanup and reconnect behavior.
- Shared UI distinguishes local inventory, peer publication and authorization.
- Packages include complete dependencies, licenses and corresponding source
  where required; driver/signing/distribution constraints are resolved.
- Automated checks and bounded failure tests pass. Physical, compatibility and
  soak results remain separately identified; the owner has deferred new device
  validation while software integration proceeds.

No percentage or claim of perfection substitutes for these gates.
