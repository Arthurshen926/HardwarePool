# ADR 1549: Interactive finite paired-store stereo capture

Status: experimental; isolated terminal/graph acceptance is mandatory.
Extends ADR 1548 without changing the Node protocol or resource authority.

## Scope and direction

The console adds `capture --config ABSOLUTE_PATH --peer NODE_UUID`, supporting
selected-source stereo capture and the existing session-owned virtual-speaker
projection. This complements playback/virtual microphone, not camera, touchpad,
incoming peer Drafts, renewal or automatic restart. Pairing-management commands
still confer no resource permission.

The command uses only the trusted fixed sibling `capyio-node linux-capture
--trusted-lab`, the four explicit paired-store selectors and the exact peer. The
actual sibling capability probe precedes terminal interaction. Existing bounded
anonymous pipes carry the public config and paired-store marker; the console
never reads protected credentials or accesses Secret Service directly. Existing
terminal foreground/no-echo, input flushing, cancellation, process trust and
bounded child cleanup remain unchanged.

Capture has a distinct strict public manifest: `sink` is the remote Sink Port,
not a local Source or playback Source. `endpoint_mode` is `selected_source`
(default) with an explicit target, or `virtual_speaker` without an external target.
The Node reports selected-source mode as `stereo_capture`; this schema conversion
is explicit rather than an alias accepting arbitrary modes. Unknown fields,
playback's `source` or proposal mode, implicit devices, invalid local/remote IDs,
non-lab addresses and invalid duration fail closed before opening the Node.
The same regular owned bounded file rules apply; filesystem latency is not a
wall-clock guarantee and config reading precedes resource launch.

## Consent and finite control

Only the current Node approval event opens a fresh prompt. The console checks the
exact configured remote Sink, local Source node, nonnil IDs, target/projection,
endpoint mode, stereo PCM format, duration and unchanged trusted-lab security
label. An owned virtual speaker must have the exact generated identity derived
from that local Source Port. It displays local Source → remote Sink, peer,
resource/mode, format, route/session and duration. The user must type the exact
current `start REQUEST ROUTE PEER` after the fresh prompt. Generic, stale,
prebuffered or mismatched consent does not authorize capture. The Node's own
current request/route validation and the counterpart's independent resource
approval remain necessary.

The warning remains explicit: UDP audio is plaintext and unauthenticated even
though control is pinned TLS. This is a trusted isolated lab path, not secure
production audio. Pairing and authenticated transport alone never imply permission
to capture or publish an endpoint.

Canonical public events use `Capture event:` and are not secret exports or an
activation claim. Capture status requires zero playback-written packets and
monotonically nondecreasing sent packets for the exact approved route. Actual
runtime state, not local consent or an endpoint queue, determines activity.
`status`, `stop`, cancellation, EOF and interruption have ADR 1548's finite bounds:
55-second preapproval envelope, 30-second consent window, selected duration plus
at most three seconds of Stop grace, bounded event framing and child reap. These
never extend the Node's grant or lease. No implicit renewal, retry or restart is
added. Stopping retains protected pairing credentials.

## Acceptance and limitations

The hosted fixture drives the actual console through an owned PTY, then the
actual Node and isolated fake Secret Service. The existing Android-policy receiver
and private PipeWire stereo-tone fixture remain the independent media oracle;
no controller shortcut writes approval directly to Node input. Selected capture
and virtual speaker each require waveform/Stop, no approval, pretyped input,
mismatched request/route/peer, expiry and authenticated remote EOF scenarios.
Exact console markers cannot substitute for independent process ownership,
protected-store read, waveform (where claimed), graph and cleanup evidence.
Expiry uses the actual negotiated finite lease, including the existing transit
reserve, rather than assuming the longer configured grant remains valid. Capture
also reserves exactly ten milliseconds before each UDP send in the existing
production owner. The fixture checks the failure observation against that exact
lease-minus-ten-milliseconds boundary before waiting, if necessary, for the
remaining lease time to prove the peer's authority is no longer live. This is not
an arbitrary early-failure tolerance or a change to the production deadline.

All 48 prior legacy, paired-store and playback-console graph profiles remain
mandatory alongside the 16 new capture-console profiles. Negative cases prove
no resource activation, while expiry/EOF prove finite teardown without claiming
full waveform acceptance. These are synthetic endpoints and identities, not
physical capture devices, user microphones/speakers, personal keyrings or real
user terminals. No production transport, backend dependency, permission policy
or lease semantics are changed.
