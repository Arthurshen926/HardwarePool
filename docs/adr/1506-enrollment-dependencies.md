# ADR 1506 dependency note

Scope: local-alpha fileless enrollment, not an independently audited security product.

- `spake2 = 0.4.0`: RustCrypto/PAKEs, MIT OR Apache-2.0, exact registry revision
  pinned by Cargo.lock. Uses this crate's Ed25519 SPAKE2 A/B suite, **not a claim
  of RFC 9382 wire compatibility**. Upstream has ongoing development but this
  release is old and explicitly has no independent security audit. Only one
  online attempt per explicitly opened, expiring PIN window is allowed.
  https://docs.rs/spake2/0.4.0/spake2/
- `ring = 0.17.14`: already transitive via rustls; explicit use for OS randomness,
  SHA-256 HMAC confirmation and constant-time verification. Existing ISC/MIT/
  OpenSSL licensing applies. No imported source modifications.
- `rustls` and `rcgen = 0.14.5`: existing repository dependencies reused for the
  normal pinned mutual-TLS channel and local certificate generation.

Alternatives considered: raw PIN hashing (rejected: offline guessing); custom
ECDH/SAS construction (rejected: unnecessary protocol invention); sending CPI
under anonymous TLS (rejected: unauthenticated key transfer); full fingerprint
QR invitation (valid future alternative, different approved interaction).

PAKE identities cover ordered public hellos, including Node IDs, names,
certificates and nonces. Direction-separated HMAC confirmation precedes TLS.
TLS uses normal CA/name verification plus the exact exchanged leaf pin before
approval or persistence messages. Each private key is generated locally.
No claim that these implementation choices substitute for external review.
