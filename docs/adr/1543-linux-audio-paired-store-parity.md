# ADR 1543: Linux audio paired-store parity

Status: experimental finite launcher integration and mandatory hosted gate.
Builds on ADRs 1538, 1540, 1541 and 1542. No physical-device claim.

## Decision

The optional `paired-store` feature now extends the existing `linux-playback`
and `linux-capture` entrypoints with the same five public selectors and strict
second-frame marker as camera and touchpad (ADR 1541). Both call the existing
exact-peer credential loader before control-listener binding. This is an
identity-source change only: no new dependency, media protocol, enrollment UI,
service, persistent approval, reconnect or renewal is introduced.

Playback and virtual microphone retain the existing `source.node_id` binding,
including the `incoming_draft` proposal mode. Capture and virtual speaker retain
the existing `sink.node_id` binding: the capture schema is normalized by its
existing parser before the shared loader compares the selected peer. Capture
does not acquire an incoming-draft mode or accept playback-only fields.

The caller must still supply the unchanged private FIFO configuration, including
an explicit finite duration and local target where required, and approve the
fresh session. The marker supplies no consent. Independent remote grants,
route/port binding, finite leases, Stop, EOF and expiry behavior are unchanged.
In particular audio media remains the existing plaintext trusted-lab UDP path;
stored identity and pinned control mTLS do not make audio media encrypted or
safe for untrusted networks. No camera Active/queue semantics change.

## Invocation

Build `capyio-node` with `--features paired-store`. Use the same selectors and
framing as ADR 1541, replacing the command with `linux-playback` or
`linux-capture`, for example:

```sh
capyio-node linux-playback --trusted-lab \
  --pairing-state /absolute/owner-only/state \
  --pairing-node 00000000-0000-0000-0000-000000000001 \
  --pairing-peer 00000000-0000-0000-0000-000000000002 \
  --pairing-bus-address unix:path=/absolute/owned/bus \
  --pairing-collection /org/freedesktop/secrets/collection/existing <&3 >&4
```

Descriptors 3/4 must be exclusively owned inherited FIFOs, not credential
files. The example identifiers are public placeholders, not generated identity.
Send the existing audio configuration frame, then exactly
`{"version":1,"credential_source":"paired-store"}`, each prefixed by its
four-byte big-endian length. Subsequent frames remain the existing local
commands, never credentials. Virtual endpoints and incoming drafts are selected
only through their existing configuration fields, not new CLI modes.

No selectors retains the original three-frame config/directory/credential
bootstrap. Partial selectors and mixed bootstrap modes fail closed. Headless
operation needs an explicit authenticated local Unix bus and already running,
unlocked persistent collection. There is no default bus lookup, autostart,
prompt, unlock, collection creation, plaintext disk fallback or journal recovery.
ADR 1540's plain local D-Bus traffic, trusted service acknowledgement, transport
size bounds and timeout/uncertain-create limitations remain unchanged. Pairing
revocation is checked at launch, not watched throughout a session.

## Acceptance and limits

The actual executable store fixtures cover audio local/remote binding,
unavailable/locked/missing/revoked or altered credentials and mixed framing
before listener/helper activity. Valid stored identities can listen and Stop
without approving any resource. Fixtures use only synthetic credentials and a
private owned fake Secret Service, never a real keyring.

The existing disposable PipeWire fixtures run legacy and paired modes against
the same generated audio and native graph owners: playback, virtual microphone,
selected-source capture and virtual speaker. Independent waveform/tone checks,
finite authorization and existing cleanup/failure scenarios remain mandatory;
loading credentials or seeing a queued buffer is not delivery evidence.
Incoming-draft coverage uses the existing control fixture, not a new claim of
incoming-draft graph coverage. Exact mode evidence prevents silently falling
back to legacy bootstrap in a feature-disabled or misconfigured test.

Local compilation and pure harness checks are not substitute evidence for
private-bus, mTLS or graph execution. Those mandatory gates run on an isolated
hosted runner when local sockets are prohibited. Passing proves software
composition only, not physical audio hardware, real keyring vendor behavior,
production secure media, power-loss durability or long-lived service readiness.
