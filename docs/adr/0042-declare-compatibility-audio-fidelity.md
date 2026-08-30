# ADR 0042: Compatibility audio backends declare fidelity instead of implying one wire

Status: accepted

## Context

ADR 0041 introduced one common media binding and packet contract, but the two
physically proven compatibility paths expose different boundaries:

- the CapyIO-authored Audio Share-compatible Broker sees decoded PCM and owns
  the private sender, while its wire strips CapyIO Route/Stream/epoch/timing
  metadata;
- MicYou owns capture, codec, FEC, jitter, network and decoded Windows handoff
  inside an external process. The CapyIO Adapter can control lifecycle but
  cannot observe a common packet or payload.

Treating both as full common-packet transports would overstate
interoperability and hide the exact migration work still required.

## Decision

Every concrete audio backend exposes a validated
`AudioTransportBackendContract` declaring:

- stable bounded backend identity;
- `StandardPort` or `AdapterManaged` interoperability;
- full-packet, PCM-payload-only or opaque-process media access;
- PCM/Opus capability;
- exact, partial, absent or opaque fidelity for Session/Route binding, Stream,
  epoch, sequence, source time, sample timeline, discontinuity, selected stream
  specification and payload;
- observable peer-authentication, confidentiality, integrity, replay and
  downgrade protection.

A `StandardPort` contract is valid only with full common-packet access and
exact metadata fidelity. PCM-payload-only access must be PCM-only with exact
payload fidelity. An opaque process must declare its payload opaque. Security
features cannot be combined into a misleading production claim.

The Audio Share v0.3.4 compatibility backend is `AdapterManaged`, PCM-payload
only and exact only for payload bytes. Its format mapping is partial and its
common identity/timing fields are absent. `AudioShareMediaSender` binds one
validated `AudioMediaStreamBinding`, rejects the wrong Stream/epoch/spec and
then deliberately submits only PCM bytes to the unchanged private sender.

The MicYou v2.0.1 backend is `AdapterManaged` and opaque to CapyIO. Its local
compatibility binding associates the external process with one conservative
voice Route/epoch for lifecycle, but no `AudioMediaPacket` API is exposed and
no common IDs are claimed on the private wire. PCM and Opus are private
capabilities, not exact CapyIO negotiation evidence.

Both compatibility contracts declare no production transport security.

## Consequences

- One common engine can now reason about backend losses without pretending the
  private protocols are mutually interoperable.
- The existing Audio Share path exercises the common packet boundary without
  changing its pinned Android wire or physical rollback path.
- MicYou remains an honest opaque golden baseline; eliminating its external
  process requires the native Android microphone and network backend rather
  than a superficial wrapper.
- UI/diagnostics can later display compatibility limitations from a typed
  declaration instead of parsing Adapter names or prose.
- No new codec, socket, Android permission, driver or third-party dependency is
introduced by this decision.
