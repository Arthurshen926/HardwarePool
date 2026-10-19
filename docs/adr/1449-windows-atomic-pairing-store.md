# ADR 1449: Windows atomic current-user pairing store

- Status: Accepted storage foundation; enrollment and connection management pending
- Date: 2026-09-18

## Context

ADR 1441 defines the bounded non-secret paired-peer directory, and ADR 1442
protects a bounded credential bundle with current-user DPAPI. Neither decision
owned file placement, atomic replacement or application restart recovery. A
Windows host therefore could not yet recover its paired-peer metadata or map an
opaque handle to protected credential bytes.

## Decision

Add `capyio-windows-pairing-store` as the Windows platform owner. It uses the
fixed current-user root `%LOCALAPPDATA%\CapyIO\Control\Pairing`, stores the
schema-1 non-secret directory as `peers-v1.json`, and accepts only credential
handles of the closed form `windows-dpapi:` followed by 64 lowercase hexadecimal
characters. Such a handle maps to one file below `credentials-v1`; handle text
cannot introduce a path component.

All reads are bounded before decoding or DPAPI access. Writes create a new file
in the target directory, flush its contents, and use Windows `MoveFileExW` with
replace and write-through flags. The one unsafe Win32 call is confined to this
platform crate and receives owned, NUL-terminated path buffers. The store
inherits the current user's local-app-data ACL and additionally relies on
current-user DPAPI for credential confidentiality and integrity; it does not
claim a custom hardened directory ACL.

A caller adding or rotating a peer must first commit every protected credential
blob and replace the non-secret directory last. A crash may leave an unreferenced
protected blob, but this ordering does not publish a new directory reference
before its blob write succeeds. The API cannot prevent external deletion or a
caller violating the ordering, so connection setup must still resolve each
handle and fail closed.

Windows Desktop reads the directory during startup and after demo reset. An
absent directory is a valid empty store and performs no write. Corrupt,
oversized or wrong-owner data is not treated as empty; the shared UI reports
the pairing store as unavailable. A ready store means only that bounded metadata
was readable, not that a peer is enrolled, every credential resolves, or an
authenticated connection exists.

## Consequences

- Windows has a restart-stable, bounded and atomically replaced persistence
  boundary for paired metadata and DPAPI-protected credential bundles.
- Opening the UI does not create a directory, enroll a peer, open a socket or
  invoke DPAPI when the directory is absent.
- Enrollment transcript, handle generation, credential rotation/removal,
  orphan cleanup, recovery/export, Android Keystore storage and the application
  connection manager remain separate gates.
