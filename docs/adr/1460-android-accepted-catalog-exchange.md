# ADR 1460: Android Runtime owns accepted catalog exchange

- Status: Accepted composition boundary; JNI listener pending
- Date: 2026-09-18

## Context

The Android Rust Runtime could own and replace its local hardware catalog, but
there was no Android-specific composition boundary for an accepted mutually
authenticated control channel. Placing admission and catalog ordering directly
in Java or a socket thread would duplicate the tested control-host invariants
and risk leaving a partially admitted Session after malformed input.

## Decision

`AndroidCatalogRuntimeV1::admit_accepted_catalog_exchange` consumes an already
authenticated server channel. It transactionally admits the first paired peer
and fresh Session through `CatalogControlLink`, follows acceptor ordering by
receiving and applying the peer catalog first, then prepares and sends the
publication-filtered local catalog.

If receive, validation, Runtime admission, publication projection or send
fails, the link is closed and the exact admitted Session is reconciled before
the error is returned. The method returns the live link only after both initial
catalog directions succeed. It performs no bind, accept, credential recovery,
discovery, reconnect or Route activation itself.

The Android JNI crate now directly depends on the existing in-repository
`capyio-control-link` and `capyio-control-tls` crates. Test-only dependencies on
`capyio-protocol`, `capyio-testkit` and the already pinned `rcgen` version build
real loopback mTLS identities and malformed-session evidence; no new external
production dependency is introduced.

## Consequences

- A future JNI listener can remain a narrow owner of socket/thread lifecycle
  and call one tested Runtime operation after authentication.
- Successful return means the initial bidirectional catalog exchange completed,
  not that later disconnect detection, leases or data-plane Routes work.
- First connection remains new-peer-only. Explicit reconnect reconciliation is
  still required before an existing peer can open another Session.
