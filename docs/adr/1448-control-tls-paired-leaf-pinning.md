# ADR 1448: Enforce paired leaf identity before control binding

- Status: Accepted
- Date: 2026-09-18

## Context

ADR 1437 records a SHA-256 peer-certificate fingerprint, but ADR 1436's TLS
channel constructors previously consumed only trust roots. A platform using a
shared issuing CA could therefore validate another leaf without enforcing the
exact certificate stored in the pairing record. Node IDs in a binding record
are not a substitute for that certificate identity check.

## Decision

Both TLS identity configurations require `expected_peer_certificate` using the
existing validated CertificateFingerprint type. There is no optional/unpinned
mode. After normal TLS 1.3 mutual certificate verification and ALPN negotiation,
the transport hashes the first authenticated certificate and compares it with
the expected leaf. Missing or mismatched leaf identity fails the connection.

The dialer completes TLS and verifies the leaf before writing any application
binding record. The acceptor verifies the leaf before reading/acknowledging the
binding. No authenticated channel reaches the catalog link on failure. Neither
side learns trust from an unauthenticated discovery announcement or a newly
observed certificate. Certificate renewal requires an explicit paired-record
update by the future enrollment owner, not silent acceptance through a CA.

## Compatibility and dependencies

This is a compile-time breaking change to the unpublished pre-alpha Rust host
configuration, not a wire-format change. Existing fixtures now provide exact
peer leaf pins. `capyio-control-tls` depends on the existing pure pairing crate
to reuse its fingerprint validation/hashing; no third-party package is added.
The alternative of duplicating hashing or leaving it to every platform caller
would permit inconsistent admission and is rejected.

## Acceptance

Real loopback mTLS tests reject wrong paired leaves on both connection sides
even with valid chains from the same CA. Exact-leaf/missing/issuer-position tests
cover the helper, and existing bidirectional catalog and lease tests retain
their success paths. No enrollment, secure storage, production trust rotation,
Android/Windows application connection or physical transfer is claimed.
