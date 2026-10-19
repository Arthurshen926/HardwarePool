# ADR 1437: Classify discovery through post-pairing records

Status: Accepted foundation; enrollment and persistence pending

## Context

LAN announcements are intentionally unauthenticated. The mTLS control transport
requires paired credentials, but there was no bounded model connecting a
discovered Node ID to platform-held credentials without placing private keys in
ordinary configuration or treating discovery as proof of identity.

## Decision

Add `capyio-pairing`, a deterministic directory of at most 64 post-pairing peer
records. Each record contains one peer Node ID, bounded display label, TLS server
name, SHA-256 peer-certificate fingerprint, pairing timestamp, state and an
opaque platform credential handle. Credential handles use a closed ASCII form;
the directory never receives private keys, trust roots or pairing secrets.

A fresh discovery candidate is classified by exact Node ID as Unpaired,
Revoked or Paired. Stale candidates are Expired. Only an Active exact match
produces a `PairedControlCandidate` containing the discovery-derived endpoint
and non-secret credential lookup metadata. Duplicate Node records fail rather
than silently replacing credentials. Explicit revocation is terminal in this
foundation model.

Certificate fingerprints reuse workspace-pinned sha2 0.10.9 (MIT OR
Apache-2.0). Alternatives were embedding DER/private material in general
settings (rejected), automatically trusting first discovery (rejected), or
using labels/IP addresses as identity (rejected).

## Consequences

The UI and platform hosts can distinguish unpaired, revoked and potentially
connectable discoveries without overstating authentication. A forged discovery
record can still cause an attempted connection, but cannot select different
credentials or bypass mTLS certificate and Session/Node binding. Actual pairing
transcript, OS secure-storage implementation, credential rotation, recovery and
cross-device revocation propagation remain future work.
