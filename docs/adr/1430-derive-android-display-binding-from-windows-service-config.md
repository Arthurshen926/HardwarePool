# ADR 1430: Derive Android display binding from the Windows service configuration

Status: accepted for unified display provisioning

Date: 2026-09-17

## Context and decision

The Android GameStream properties repeat the Windows service's selected
endpoint, Adapter/Capability/Session/Route/Stream identities, epoch, virtual
mode, codec, dynamic range, bitrate and private port namespace. Maintaining a
second hand-written file can create a valid-looking bundle that authentication
later rejects or that starts a different media selection.

The Windows service crate now exposes a metadata-only converter and a narrow
CLI. It reads at most 16 KiB and uses the same deny-unknown-fields schema-3
parser and semantic validation as service startup. During conversion its
credential callback returns a one-byte placeholder, so the operation validates
credential paths but never reads or serializes the Windows server private key.
It emits exactly the 17 fields accepted by the Android GameStream loader, with
numeric codec/range values derived from the validated typed session.

`new_android_gamestream_bundle_from_display_config.ps1` captures this output
and passes it in memory to the bounded bundle writer from ADR 1429 together
with an explicitly selected client credential set. The writer makes no
temporary plaintext properties file and refuses to overwrite an output bundle.
The resulting ZIP does contain the Android client private key and must be
handled as a credential, not as an ordinary diagnostic artifact.

## Evidence and limits

A Rust test checks all derived selection fields and rejection of an unsupported
codec. Strict Clippy and the Windows service test suite cover the converter.
The wrapper produced a five-entry bundle from the prior successful schema-3 lab
configuration; entry names and sizes were inspected without logging contents.

This does not enroll or rotate credentials, prove that a selected client key
matches its certificate, transfer the bundle securely, update the installed
Windows service, start a display Route or present frames. The prior certificate
is experimental and bound to the existing lab CA and Windows endpoint.
