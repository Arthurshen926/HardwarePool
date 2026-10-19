# ALPHA-INVENTORY — Exact read-only application inventory

Status: completed
Owner: Codex
Created: 2026-10-19
Requirements: NFR-MAINT-002, NFR-MAINT-005

## Objective

Prevent the portable component report from claiming bundle integrity for empty,
unlisted, linked or size-mismatched application inputs. This is a Stage 4
validation correction, not a new installer or an architecture/protocol change.

## Scope and constraints

Change only the read-only schema-v3 application inventory check and its fixture
tests/documentation. Preserve missing/mismatched artifact reporting and existing
Core/integrated artifact names. The root manifest is metadata, not an artifact;
no other extra files are implicitly allowed. Producers already record bytes and
SHA-256. Schema-v1 unified Windows packages and Extras remain separate contracts.
No dependencies, device access, credentials, deployment, publishing or system
configuration changes. Physical acceptance and signing remain unproven.

## Acceptance and evidence

- A nonempty exact ordinary-file inventory passes with valid bytes and hashes.
- Empty/duplicate/unsafe paths, links and extra files fail closed.
- Missing files, wrong lengths and hashes never produce bundleIntegrity=true.
- Temporary-directory tests cover nested files, uppercase hashes and both
  missing and malformed input; no payload is executed.
- Run Python unittest discovery, repository validator/self-test and diff check.
- Record unavailable Rust/Windows/full Alpha gates explicitly.

## Baseline

At eea515dfde4037414706f12b0411be3cc46ef5c9, repository validation passed with
88 Requirement IDs; parser self-tests and all 11 Python tests passed. Rust tools
are unavailable in this cloud environment. No hardware or installed package was
inspected.

## Completion record

Implemented strict nonempty inventory, byte/hash validation, canonical relative
paths, case-insensitive duplicate detection, link rejection before file reads and
unlisted-file rejection. Hashing streams file contents. Existing missing/changed
artifact states and read-only component plans are preserved. Both schema-v3
producers were reviewed; their byte counts and manifest exclusion match this
check. No public protocol or runtime boundary changes require an ADR.

Validation on the cloud checkout:

- Python unittest discovery: 25 discovered, 24 passed, one skipped (the real
  Windows junction fixture requires Windows). Portable reparse-attribute and
  pre-read rejection tests passed; no real Windows result is inferred.
- Repository validator: passed, 88 Requirement IDs; parser self-tests passed.
- `git diff --check`: passed.

Not run: Rust checks/full alpha-check (Rust toolchain absent), Windows PowerShell
package/installer tests, frontend/Android builds, physical media and new-machine
acceptance. No installed candidate hashes or historical acceptance claims changed.
Concurrent filesystem mutation is outside this read-only inventory check; it is
not a replacement for installer-time verification or publisher signatures.
The selected root and its entries reject links/reparse points; existing ancestor
aliases resolve to that root, and are not additional inventory authority.
