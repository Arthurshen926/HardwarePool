# ADR 1459: Android pairing readiness admits active credentials

- Status: Accepted host admission; control listener pending
- Date: 2026-09-18

## Context

ADR 1452 made `AudioNodeService` own the canonical paired directory, but its
`ready` state proved only that the non-secret document was readable. A stale or
missing Keystore credential could therefore remain hidden until a future
connection attempt. Revoked peers also had to remain audit records without
causing secret recovery.

## Decision

`capyio-pairing` exposes an iterator over active records while preserving the
complete record iterator for audit and UI counts. The Android JNI boundary
decodes the canonical directory again and emits only active credential handles
as a bounded newline-delimited byte projection. Credential handles cannot
contain newlines under the normative pairing schema.

At Service startup Android loads and validates every referenced active
credential through `AndroidCredentialStore`, then immediately overwrites the
temporary plaintext byte array. A revoked record is never recovered. Any
directory, JNI, handle, Keystore, AEAD or typed-bundle failure reports pairing
state `failed`, retains no directory in admitted host state and leaves local
hardware functions available. An empty directory remains `ready`.

This is a startup consistency gate, not enrollment or authentication. It does
not expose handles or credentials to the WebView, open a socket, advertise a
discovery endpoint, create a Session, or prove peer possession of a key.

## Consequences

- Android `ready` now means the directory and all active control credentials
  are recoverable and typed, rather than merely that metadata is readable.
- Revoked credentials can remain stored for explicit later cleanup without
  being touched during startup.
- Keystore recovery currently occurs serially during Service creation; the
  bounded maximum is 64 peers. A future connection manager may cache only
  non-secret resolved metadata and must recover secret material on demand.
