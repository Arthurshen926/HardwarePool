# ADR 1453: Typed bounded control credential bundle

- Status: Accepted storage-to-TLS carrier; enrollment pending
- Date: 2026-09-18

## Context

The paired directories already retain closed platform credential handles, and
Windows DPAPI and Android Keystore stores protect bounded byte arrays. The
control TLS boundary, however, separately expected a certificate chain,
private key and peer-specific trust root. With only opaque storage bytes there
was no shared format connecting these boundaries, and arbitrary protected bytes
could be mistaken for a usable control identity.

## Decision

`capyio-control-credentials` defines one dependency-light binary carrier for an
already-issued control identity. It contains one to four non-empty DER
certificate-chain entries, one non-empty DER private key and one non-empty peer
CA certificate. A magic/version marker, zero reserved bytes, big-endian lengths,
exact consumption and a 64-KiB total limit make malformed, extended, empty and
oversized input fail closed.

The type owns and clears its component vectors on drop. Decode builds directly
inside that guarded value so a later parse failure also clears components
already copied. Encoding returns a caller-owned plaintext vector; platform
stores must protect it immediately and clear it after use. This bounds ordinary
process-memory exposure but does not claim forensic erasure of allocator copies,
OS buffers or data already handed to TLS libraries.

The Windows pairing store now offers typed put/load methods. It clears encoded
and decrypted temporary plaintext and rejects a valid DPAPI blob whose payload
is not this format. The Android store validates through Rust JNI before creating
a Keystore key or mutating a file and validates again after authenticated
decryption, so pre-format legacy plaintext fails closed. The control TLS server
and client identity constructors consume a decoded bundle while retaining the
separate exact paired-leaf fingerprint from the non-secret directory.

This is neither a wire protocol nor an enrollment transcript. It does not issue,
rotate, recover or revoke certificates and does not choose which peer is trusted.

## Consequences

- Windows and Android protect the same closed credential semantics while keeping
  platform-specific at-rest mechanisms.
- Stored arbitrary bytes can no longer flow through the typed path into control
  TLS configuration.
- Enrollment must produce the exact bundle and commit credential first,
  directory last; connection owners must resolve it only for the selected peer.
- Existing untyped Windows storage primitives remain an internal lower-level
  mechanism and provide no admission on their own.
