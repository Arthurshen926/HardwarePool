# ADR 1479: Show paired peer metadata on Android without granting access

- Status: Accepted for the integrated alpha
- Date: 2026-09-19

## Context

The shared Remote Resources screen listed paired peers on Windows but always
received an empty paired-peer list on Android, despite the Android Service
holding the same Rust-validated non-secret pairing directory. This made a
known Windows node invisible on the tablet and conflated an empty UI with
missing trust.

## Decision

Android projects only Node ID, display name and revoked state from its
already-canonicalized pairing directory through a narrow WebView bridge. The
shared UI reuses the same bounded paired-peer parser as Windows and shows the
metadata in Remote Resources. It distinguishes paired from currently
authenticated: only the single-active-peer Android alpha case may associate
the existing connected control status with a displayed peer. No credential
handle, certificate fingerprint, private key or raw directory enters the UI.
Malformed/unavailable directory state returns an empty list, not inferred
trust. This is a read-only view; it does not create a Route or initiate a
control connection.

## Consequences and verification

The screen now names a paired Windows node on Android, but Windows remains the
control dialer and Android the listener. The UI explicitly states that Android
cannot yet browse Windows' shared catalog or initiate a Windows connection.
The existing shared parser tests reject extra credential fields, malformed IDs,
duplicates and overlarge lists. APK compilation and physical WebView readback
verify the bridge returns the expected three-field projection. This is not
evidence of bidirectional resources or data-plane success.
