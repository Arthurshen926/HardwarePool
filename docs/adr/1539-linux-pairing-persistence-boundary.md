# ADR 1539: Linux pairing persistence boundary (core only)

Status: experimental implementation. ADR 1540 supersedes the core-only backend
availability and atomic/durability assumptions below; this document records the
original boundary.

## Context and decision

Linux finite IPC launchers already accept one approved paired directory and one
issued control credential bundle through inherited private pipes. This slice
adds `capyio-linux-pairing-store`, not a background Node service, enrollment CLI,
credential generator, lease renewal mechanism, or launcher permission bypass.

The caller explicitly supplies an existing absolute current-user 0700 directory
and a `CredentialBackend`. No native backend is shipped. A plaintext JSON/file
backend is not an acceptable implementation. Until a native protected backend is
implemented and independently tested, this API is an integration boundary only;
it does not establish real Linux secure persistence or user-keyring support.
Tests use only an in-memory synthetic backend and synthetic credential bytes.

Handles use the shared enrollment manager's `linux-protected:` prefix followed
by the SHA-256 digest of the exact bundle. Duplicate digest keys are rejected,
and replay never recreates a missing committed credential. Public `revoke` retains
a tombstone and the protected key; stale recovery cannot reactivate it.

The store holds a nonblocking exclusive OS file lock for its entire lifetime.
It records the creating process ID: inherited store operations after fork are
rejected before filesystem/backend/manager access, and a child dropping its
inherited descriptors never explicitly unlocks the parent's shared description.
The owner explicitly unlocks on drop so transient fork-before-exec descriptors
do not delay normal reopening. After fork, exec or reopen rather than using the
inherited store. A harness-free, single-threaded subprocess regression verifies
child Drop cannot release the live parent lock.
All path components are opened with no-follow directory descriptors; metadata
operations are relative to the opened root. Regular files must belong to the
current effective user, have one link, and have 0600 permissions. It never fixes
permissions, follows symlinks, unlocks a collection or changes system services.
The directory must be provisioned deliberately by its owner. Same-user arbitrary
code and replacement of the store/lock namespace are outside this boundary.
Use a local filesystem with reliable flock, atomic rename and directory fsync.
Input sizes/counts are bounded; filesystem/backend latency is not time-bounded.

## Transaction and recovery

1. Decode the target-bound invitation and require one active peer, a Linux
   protected handle, no duplicate handles, no changed existing record or pin,
   and no different pending journal.
2. Ask the protected backend for atomic create-only/byte-exact idempotent put,
   then read back and validate the bounded credential and its SHA-256 digest.
3. Write only owner-bound public peer metadata and the credential digest to a
   temporary file, fsync, atomically rename the journal and fsync the directory.
4. The normal manager calls commit only after remote durable preparation.
   Commit revalidates the original journal/credential and existing records,
   atomically/fsync commits metadata, then removes/fsyncs the journal.

`poll_enrollment` composes `stage_with`, `persist_with` and `fail_persistence` in
one caller-driven iteration. It never starts a thread or a pairing window.
Explicit `recover` replays only the local journal and reports `recovered` through
`note_local_recovery`; it never claims remote completion or issues new identity.
It refuses recovery while the manager is running a pairing attempt.

A crash before journal durability may leave an unreferenced protected credential;
only replay of the original approved invitation can complete that stage. The
store deliberately has no credential-delete operation: neither rollback nor
recovery can erase an existing credential. A crash after directory commit but
before journal removal is idempotently recoverable. Orphan cleanup, disk-quota
policy, native backend transactions and external revocation management remain
separate work. Stale temporary files are ignored, not adopted or deleted.

Documents bind the exact local Node ID and every credential handle to the original
bundle digest. A missing, locked, duplicate, altered or malformed credential fails
closed. A corrupt/oversized/pending journal blocks bootstrap until explicit local
recovery. Revoked peers cannot load credentials. Public metadata integrity against
a compromised owner account is not claimed; TLS still validates key/certificate
parsing, ordinary mTLS and the stored exact peer leaf at the existing boundary.
Secret buffers are not Debug/Serialize/Clone and best-effort cleared on drop;
compiler copies, the shared invitation representation, swap and dumps are not
an audited secret-erasure guarantee.

## Typed bootstrap boundary

`bootstrap(exact_peer_id)` reads the committed directory and returns only that
active peer plus its exact bounded issued credential, even when other peers are
stored. `BootstrapMaterial::directory_json()` and `credential_bytes()` supply the
existing launcher's second and third frames. The launcher must retain exclusive
private inherited-pipe ownership and all existing framing/deadline validation.
This crate does not write the pipes or launch any process. The first typed launch
configuration, explicit target/duration, resource consent, fresh Session, route
binding and finite leases remain unchanged. No argv/environment/file secret
transport or default backend fallback is added.

## Dependencies and validation

Reuses workspace serde, serde_json, sha2, thiserror, capyio-pairing,
capyio-control-credentials and capyio-enrollment. Reuses pinned rustix 1.1.4
(Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT) for safe Linux directory-fd,
no-follow file I/O and flock; enables its process feature for effective-UID
validation. Rustix is already maintained upstream and used by Linux host crates.
The harness-free fork regression reuses pinned libc 0.2.189 (MIT OR Apache-2.0)
as a test-only dependency for fork/waitpid; production code remains unsafe-free.
Alternative std-only paths would weaken directory-relative race handling;
handwritten unsafe libc wrappers would duplicate maintained bindings.

Run `cargo test --locked -p capyio-linux-pairing-store` and
`cargo clippy --locked -p capyio-linux-pairing-store --all-targets -- -D warnings`.
Synthetic unit tests are not native Secret Service, physical device, real
credential, power-loss filesystem or long-running service acceptance evidence.
A future native adapter must use exact namespace attributes, reject duplicate
matches and locked collections, avoid implicit unlock/create, and be validated
against an isolated fake service without touching an existing user keyring.
