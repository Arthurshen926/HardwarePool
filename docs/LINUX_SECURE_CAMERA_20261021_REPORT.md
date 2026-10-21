# Finite secure camera software candidate — 2026-10-21

Base: `30d0a59058b5e2426652fa5f3e16ee1c9ffb0457` on
`codex/linux-ipc-validation-20261019`. No publication or deployment by this report.

## Implemented boundary

- Distinct versioned secure offer, actual dynamic port, authenticated socket peer
  address, pinned TLS 1.3 camera ALPN and complete Route binding/ACK.
- One-shot camera listener, both finite endpoint leases, existing Ready Session
  and exact independent consent; no helpers before data admission.
- Caller-polled Linux owner, rendezvous records, control supervision during
  decoder waits, source/sink/grant/lifetime deadlines, terminal cleanup.
- Explicit CAVC-to-NV12 projection boundary without relabeling negotiated Routes.
- Actual Android Rust camera policy and existing paired-credential sender seam;
  secure offers are quarantined from the plaintext Java path.
- Existing plain Windows lab camera and existing transport wire contracts remain
  unchanged. Existing reusable TLS listener semantics are preserved.

## Local verification

- Combined targeted Rust suite: 223 tests passed across control TLS, protocol,
  control link, Android JNI crate, Linux camera and Linux AVC.
- TLS suite includes 101 tests and actual loopback handshake/record traffic,
  exact binding/pin/ALPN rejection, plaintext/oversize, partial I/O cancellation,
  one-shot/reusable listener compatibility and lease deadline restrictions.
- Native-feature compilation and strict Clippy passed; no warnings accepted.
- Both native-feature driver tests passed: real Android consent and pinned-control
  Source lease acquisition/secure-offer sequencing, with no early media children.
- Documentation traceability and Adapter manifest validation passed.
- Python repository suite: 71 tests passed with two platform-specific skips,
  including exact marker omission/spoof rejection for negative graph scenarios.

## Required hosted acceptance

`Linux secure camera validation` adds eleven actual private-graph scenarios:
pixels, no_consumer, expiry, control_stop, control_eof, partial_cavc, source_revoke,
sink_revoke, child_exit, decoder_exit, server_disconnect. It requires actual
Android consent/lease policy and Rust sender, pinned control/data, existing CAVC
decoder, independent native NV12 pixel/order consumer, helper reaping and graph
sentinel continuity. Exact verification markers prevent negative-driver assertion
failures from masquerading as expected terminal outcomes.

Local graph execution stopped before launching a graph because `/usr/bin/pipewire`
is absent. No security setting, permission bypass, daemon install or remote-device
fallback was attempted. Hosted graph results remain pending for this candidate;
compilation and TLS tests are not composed-camera evidence.

## Not implemented or claimed

Java/Camera2 complete-record delivery into the Rust secure sender is not yet
wired. This is a policy/synthetic software-session slice, not a complete Android
secure camera capture feature or merely an untested physical path. The next
implementation increment must add that service-owned bridge before device tests.
No new credentials/enrollment, camera permission, APK installation, kernel/V4L2
change, general portal/browser compatibility or physical-device acceptance.
