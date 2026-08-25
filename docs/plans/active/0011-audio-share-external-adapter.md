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
   address/port/endpoint, type startup/exit failures, and stop/reap the child
   idempotently.
4. `CAPY-AUDIO-001A2A`: observe process-owned Windows TCP receiver presence
   without parsing ordinary logs, while labeling it weaker than playback
   health.
5. `CAPY-AUDIO-001A2B`: bind the controller to one Runtime-owned
   `AdapterManaged` Route with fresh retry epochs and independent IMU state.
6. `CAPY-AUDIO-001A3`: expose the Route through a generic versioned Quick
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

## Progress

- `CAPY-AUDIO-000`: complete.
- `CAPY-AUDIO-001A0`: complete and pushed in commit `4065618`.
- `CAPY-AUDIO-001A1`: complete locally. The supervisor proves child process and
  TCP-listener lifecycle only.
- `CAPY-AUDIO-001A2A`: complete locally. Windows IP Helper reports only
  established rows owned by the supervised process and explicit port; no
  addresses or log prose enter the result. This proves receiver TCP transport,
  not negotiation, UDP PCM, `AudioTrack` or audibility.
- `CAPY-AUDIO-001A2B`: complete locally. A desktop composition controller binds
  the process boundary to one Runtime-owned `AdapterManaged` Route, gates
  activation on three consecutive receiver samples, reports receiver/process
  failures as typed `Offline` Problems, advances explicit retry epochs and
  preserves unrelated active IMU Route state.
- `CAPY-AUDIO-001A3`: complete locally. The schema-v1 generic
  Quick Action exposes truthful configured/blocked and Route lifecycle states,
  finite start/retry/stop operations and bounded Problems. Trusted host
  environment owns executable/network/endpoint configuration, and a host worker
  polls independently of the WebView. The physical lab passed Active epoch 1,
  disconnect/Offline epoch 2, retry Active epoch 3 and explicit stop; a separate
  run delivered a Windows system WAV to a 48 kHz stereo Android Track with
  non-zero written frames.
- An authorized audible repeat established the Android TCP/UDP peer, advanced
  the active 48 kHz stereo Track from zero to 2,421,542 server frames across six
  system WAV plays, and was clearly heard by the user beside the phone. Cleanup
  left no process/listener. The result is one audible functional case, not a
  latency, quality, background or soak claim.
- Post-slice hardening bounds receiver startup at 120 host polls, reaps on
  exhaustion and reports a retryable typed Problem. RDP endpoint testing showed
  upstream format fallback, so the Route now declares a private-negotiated
  format rather than fixed signed-16/48 kHz. A 15-second screen-off run retained
  TCP and advanced the Android Track by 757,150 frames with clean shutdown;
  secure lock, longer background, focus and soak remain open.
- Start-time inventory probing now maps a disappeared configured endpoint to
  the sanitized, retryable `CAPY.AUDIO_SHARE.ENDPOINT_UNAVAILABLE` Problem.
  The endpoint ID is neither parsed from logs nor returned to the WebView;
  enumerated trusted-host reselection remains a later UX slice.
- Real ignored probes confirmed that the current RDP endpoint starts and reaps,
  while the disappeared former endpoint is rejected as
  `ConfiguredEndpointMissing` before spawn with supervisor state still stopped.
