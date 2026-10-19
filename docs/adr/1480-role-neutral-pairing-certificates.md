# ADR 1480: Role-neutral control certificates for newly paired Nodes

- Status: Accepted for new integrated-alpha pairings only
- Date: 2026-09-19

## Context

CapyIO Nodes have no global client/server role, but the current offline
invitation issues a Windows leaf restricted to TLS ClientAuth and an Android
leaf restricted to ServerAuth. Reversing the control TCP direction with those
credentials fails TLS role validation before any catalog exchange. Retrying or
weakening verification would hide this incompatibility, not solve it.

## Decision

Newly issued control leaf certificates carry both TLS ClientAuth and ServerAuth
extended key usages. Each Node keeps its own unique leaf and private key, with
the exact opposite leaf fingerprint and dedicated CA pinned in the existing
pairing directory/credential bundle. The fresh Session binding still includes
the ordered server and client Node IDs. Any future inbound or outbound control
implementation must authenticate the same stored peer identity and separately
admit each capability/Route; dual EKU is not a resource grant.

The invitation format, directory schema and credential bundle encoding remain
unchanged. Existing installed pairs retain their original role-specific
certificates and are **not** silently replaced, auto-trusted or claimed to be
reverse-connect capable. A deliberate, recoverable re-pair/rotation flow is
required before those pairs can use reverse dialing.

## Verification and remaining work

A loopback test issues a new pair and completes pinned TLS 1.3 mTLS and fresh
Node/Session binding in both TCP directions using its actual encoded bundles.
This proves the certificate policy, not Windows listening, Android outbound
dialing, simultaneous-connect arbitration or physical device behavior. Those
remain subsequent slices, along with explicit old-pair migration.
