# ADR 1469: Target-bound offline pairing invitation

## Status

Accepted for the integrated alpha.

## Context

The authenticated control link previously required credentials to be provisioned
outside the product. Ordinary users need a bounded first-pairing path before
discovery and automatic reconnect can operate.

## Decision

Windows may issue one private CA and two short-scope mTLS leaf identities for one
explicit Android Node ID. It commits the Windows client credential to the current
user DPAPI store and writes a `CPI1` invitation containing the Android server
credential plus its non-secret paired-peer directory. The invitation is bound to
the exact target Node UUID, has fixed length fields, rejects trailing data and is
limited to 131,100 bytes.

Android imports the file through the system document picker. Rust validates the
container and target binding; Java stores the typed credential with Android
Keystore before atomically committing the directory, then hot-reloads the
control listener. Credential bytes do not cross the WebView bridge.

The desktop uses pinned `rcgen 0.14.5` with its `ring` backend. This is already
present in the workspace test graph, is MIT/Apache-2.0 licensed, maintained, and
avoids invoking an external certificate tool. OS certificate enrollment and a
cloud CA were rejected because they expand privilege and service scope.

## Consequences

- First pairing now has a product UI without weakening post-pairing leaf pinning.
- The invitation contains private key material until imported. The UI instructs
  the user to transfer it privately and delete it after a successful import.
- This alpha does not yet provide QR/PAKE confirmation, expiry, rotation,
  revocation synchronization or recovery. It is not production-grade enrollment.
- Android intentionally retains the current single-active-control-peer limit.
