# CAPY-AUDIO-000/001A — Audio Share spike and external Adapter

Status: active

Owner: Codex

Created: 2026-08-24

Requirements: `FR-SCEN-002`, `FR-ADAPTER-002..005`, `FR-ROUTE-003..005`,
`FR-DIAG-001..004`, `NFR-STAB-001..004`, `NFR-MAINT-004`

## Objective

Wrap a pinned, unmodified Audio Share release behind a bounded CapyIO Adapter
boundary and prove one Windows system-playback to Android speaker
`AdapterManaged` Route without importing or rewriting the upstream PCM data
plane.

## Slices

1. `CAPY-AUDIO-000`: record provenance, verify release hashes, characterize the
   CLI and Android receiver, and retain separately authorized physical playback
   evidence.
2. `CAPY-AUDIO-001A0`: implement deterministic configuration, version and
   playback-endpoint parsing plus a bounded executable probe.
3. `CAPY-AUDIO-001A1`: supervise `as-cmd` without a shell, bind an explicit
   address/port/endpoint, translate exit or receiver loss into a structured
   Problem, and stop/reap the child idempotently.
4. `CAPY-AUDIO-001A2`: bind the controller to one Runtime-owned
   `AdapterManaged` Route with fresh retry epochs and independent IMU state.
5. `CAPY-AUDIO-001A3`: expose the Route through a generic versioned Quick
   Action projection and repeat the authorized physical playback/disconnect
   test.

## Acceptance

- the exact Audio Share release, revision, license and binary hashes are
  recorded before integration code is committed;
- configuration rejects implicit/default network selection, zero ports,
  unbounded endpoint identifiers and shell command strings;
- probe and process output have byte/line/deadline bounds and never treat
  ordinary upstream logs as CapyIO Sidecar JSON-RPC;
- the upstream TCP/UDP PCM path stays outside CapyIO control messages;
- receiver loss produces a stable Route-related Problem and `Offline` state;
- explicit retry uses a later epoch and explicit stop reaps the child;
- stopping/failing the speaker Route does not mutate the IMU Route;
- all hardware-free tests and repository gates pass.

## Excluded

- vendoring or modifying upstream source;
- distributing the unsigned upstream Windows binary or APK;
- a CapyIO-owned Android application, automatic APK install or permission
  changes;
- microphone sharing, dedicated virtual render endpoint, codecs, production
  pairing/encryption, WAN relay or automatic retry policy;
- claiming subjective audible quality without a person beside the receiver.

## Risks

- the upstream Windows release binary is not Authenticode signed;
- Windows system-loopback capture and protected content vary by endpoint and
  application;
- Android background, audio-focus and power-management behavior requires
  retained physical evidence;
- Audio Share's own protocol is an Adapter-private contract and does not imply
  interoperability with other audio Adapters.
