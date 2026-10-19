# ADR 1472: Explicit touchpad-owner consent on the authenticated control link

## Status

Accepted for the user-requested touchpad integration slice; data-plane wiring
and online device pairing remain separate work.

## Context and normative change

ADR 1471 previously allowed Android publication selection to authorize every
resolving Route. The requested touchpad workflow instead requires publication
and individual use consent to be separate. This decision supersedes that policy
for local Android Touchpad Source Routes only; other alpha capabilities retain
their existing policy in this bounded slice.

## Decision

- An admitted touchpad proposal is mirrored without authorization and returns
  Pending. A maximum of 16 consent requests is exposed by the local trusted UI.
  Each expires after 60 seconds; retransmission cannot extend its deadline.
- Allow applies only to that exact Route and Ready Session and expires after
  five minutes. Reject/timeout never grants authority. No persistent remembered
  authorization is added. Disconnection and catalog removal invalidate requests.
- Windows reuses a compatible Draft request after Pending, rather than creating
  a new prompt on each button press. The user retries after owner confirmation.
  Denied proposals remain denied; a new request requires an explicit new action.
- Pending and denied responses do not authorize the local mirror or acquire a
  lease. Finite grants bound the requested lease TTL. Actual occupancy still
  belongs to the owner and is not inferred from consent or catalog visibility.
- No wire field or protocol version changes. No continuous payload, endpoint
  configuration, driver deployment or OS input is introduced by this slice.
- The shared UI reports that approval is control authorization, not a started
  touchpad data channel. Windows-side consent for reverse initiation is pending.

## Dependency note

The Android JNI crate uses the existing workspace-locked `serde` / `serde_json`
versions for bounded consent observation JSON. Both are already maintained
workspace dependencies under MIT/Apache-2.0; no new downloaded package/version
is introduced. Typed serialization avoids custom string escaping. A custom
binary DTO was considered but would duplicate parsing in Java and TypeScript.

## Verification

Host tests cover Pending lease rejection, finite/single-use approval, denial,
expiry, repeat-request deadline retention, Session loss and queue limits. Shared
UI parser tests reject oversized, duplicate and malformed requests. ARM64 JNI,
Java, APK and physical request/decision checks remain separate evidence.
