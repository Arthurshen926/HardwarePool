# ADR 1470: Manual authenticated control endpoint fallback

## Status

Accepted for the integrated alpha.

## Context

IPv4 multicast discovery is not carried by many overlay VPNs, guest networks or
AP-isolation configurations. A paired device must remain reachable without
turning a typed IP address into authority.

## Decision

The Windows Settings page may accept an exact Node UUID, numeric IP address and
control port. The backend resolves credentials and the pinned peer leaf only
from the existing active paired directory, then runs the identical mTLS Node
binding, catalog admission and heartbeat path used by discovered candidates.
Unpaired, revoked, nil, multicast and unspecified targets fail closed.

Android publishes its current ephemeral control listen port in the shared
control-plane status after pairing. The port is routing metadata, not a secret.

The Settings connection form selects bounded non-secret paired-peer metadata
(Node ID, label and revoked state) from the native directory rather than requiring
users to transcribe a UUID. Revoked records cannot be selected; the backend still
revalidates trust for every attempt. Credential handles, fingerprints and private
material are not exposed by this inventory. Both manual and discovered connection
attempts run on a blocking worker, not the Tauri UI thread. The form distinguishes
the CapyIO control port from Android's unrelated ADB port and keeps legacy address
preferences separate from actual connection actions.

## Consequences

- One manual IP/port entry works on networks where multicast is unavailable.
- The fallback does not bypass pairing, certificate pinning or Session binding.
- Port persistence was initially a UI/user concern. ADR 1476 adds a bounded
  post-authentication endpoint hint for repeat connections, but endpoint
  migration without discovery or new manual input remains open.
