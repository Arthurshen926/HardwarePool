# ADR 1452: Android Service owns paired-directory readiness

- Status: Accepted read-only host admission; enrollment pending
- Date: 2026-09-18

## Context

ADR 1451 delivered atomic paired-directory persistence but deliberately left it
detached from the long-lived unified Service. The shared UI therefore could not
distinguish an available empty Android store from a missing implementation or a
corrupt trust document.

## Decision

`AudioNodeService` constructs the store only after recovering its stable local
Node UUID. Startup loads and canonicalizes the complete directory and obtains
its bounded peer count through the shared Rust parser. Success reports pairing
storage `ready`; absence is valid ready state with zero peers. I/O, schema, JNI
or identity failure retains no document, reports `failed` with zero peers and
does not prevent local hardware catalog/lifecycle operation.

The Service retains the canonical non-secret bytes as the future connection
manager's input. It exposes only the state and count through its existing local
Binder. `AndroidUiSnapshotJson` validates the state/count combination and the
shared schema-5 UI displays it separately from transport and catalog exchange.
The disconnected Activity fallback remains `not_configured` because there is no
bound Service owner to attest to the store.

No write/enrollment Binder method is added. Ready means readable storage only;
it does not resolve every referenced credential, authenticate a peer, create a
Session or start discovery.

## Consequences

- Android and Windows now surface comparable pairing-store readiness semantics
  through the same shared UI contract.
- Corrupt Android trust metadata is visible as unavailable rather than silently
  becoming empty or crashing unrelated local hardware functions.
- Enrollment, credential-first/directory-last commit orchestration, revocation,
  authenticated connection ownership and reconnect remain pending.
