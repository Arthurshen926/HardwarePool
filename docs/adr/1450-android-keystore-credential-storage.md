# ADR 1450: Android Keystore-backed control credential storage

- Status: Accepted storage primitive; enrollment and host wiring pending
- Date: 2026-09-18

## Context

The shared paired-peer model retains only an opaque platform credential handle.
ADR 1449 supplies the Windows implementation, but the unified Android app had
no equivalent boundary. Storing a TLS private-key bundle directly in
`SharedPreferences`, ordinary app files or the WebView would make the handle
abstraction ineffective and would not bind at-rest protection to the device's
Android Keystore.

## Decision

Add `AndroidCredentialStore` inside the unified app. It accepts only handles of
the form `android-keystore:` followed by 64 lowercase hexadecimal characters.
The suffix selects both a fixed Keystore alias and one fixed file below the
app's no-backup `pairing/credentials-v1` directory; it cannot introduce a path
component.

For each handle, Android Keystore owns a non-exportable 256-bit AES key allowing
encrypt/decrypt only with randomized AES-GCM and no padding. The app stores one
versioned envelope containing a 12-byte random IV and authenticated ciphertext
through `AtomicFile`. Plaintext input is non-empty and at most 64 KiB; envelope
reads and every encoded length are bounded before allocation or decryption.
Missing keys, malformed envelopes and authentication-tag failures fail closed.

Removal deletes the Keystore key before deleting the ciphertext. An interruption
may leave an unreadable ciphertext file, but cannot leave readable credentials.
Replacement may leave an unused key after a failed first file write. Enrollment
and recovery code must handle these fail-closed remnants; this slice does not
enumerate or clean unrelated aliases.

No WebView method, permission, foreground-service declaration or automatic
startup path is added. The shared UI continues to report Android pairing as not
configured until a later enrollment owner stores a bounded paired directory and
wires handle resolution into the authenticated connection manager.

## Consequences

- Android now has a real device-backed confidentiality/integrity primitive for
  future control credentials without a new production dependency.
- The test APK uses a single dedicated handle, checks replacement and tamper
  rejection on a physical device, removes the key/blob, and is then uninstalled.
- Paired-directory persistence, enrollment transcript, credential rotation,
  connection management and release-device qualification remain separate gates.
