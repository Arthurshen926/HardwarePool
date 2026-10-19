# ADR 1433: Local preferred peer address profile

Status: Accepted

## Context

Existing lab features repeatedly ask for the same Windows private/overlay IPv4,
although their ports, tokens, credentials and lifecycle remain independent.
Authenticated discovery and unified pairing are not yet available.

## Decision

The shared Vue client may persist one versioned preferred-peer IPv4 in its
origin-local storage. The exact two-field JSON accepts only canonical RFC1918
or CGNAT/overlay IPv4 text and is limited to 128 encoded bytes. Unknown fields,
public/loopback addresses, alternate numeric forms and incompatible versions
fail closed. Storage failure leaves all hardware controls usable and reports an
error instead of silently claiming persistence.

The address only pre-fills forms that lack a native per-feature saved peer.
Android audio's native saved direction remains authoritative and replaces the
suggestion on readback. The Android controller form adopts a changed preference
only while it is empty or still equals the previous preference. Clearing the
profile does not delete native audio settings or tokens.

Tokens, credentials, ports, grants and hardware state are never stored in this
profile. Saving it performs no network I/O, pairing, discovery, Route mutation
or lifecycle operation.

## Consequences

Users can enter the common peer IP once for current trusted-network workflows,
without implying that feature-specific protocols have been unified. Future
authenticated discovery may replace the suggestion after explicit enrollment;
it must not reuse this local preference as peer identity evidence.
