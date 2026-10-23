# ADR 1571: Explicit Linux bilateral secure stereo entry

Status: Candidate

## Scope and compatibility

This slice activates a separate experimental selected-source path through
`capyio-node linux-capture --trusted-lab` and the Linux console, using explicit
paired-store selectors and `media_security: pinned_tls13_stereo_v1`. It extends
ADR 1570's library-only scope; its consent, receipt and owner invariants remain
normative. The nonsecret configuration selects an exact PipeWire target and a
1–60 second duration. `auto`, `default`, omitted target, virtual-speaker mode,
mono formats and caller-selected nonzero media ports are not alternatives.
The Android marked path is one attempt per `AndroidCatalogRuntimeV1`: another
marked connection requires a fresh Runtime and control Session. Consumed B1
lineage/tombstones are not automatically reset; this candidate does not offer
repeat sessions in the same Android Runtime. The default host is still noncapturing. Legacy UDP, secure mono/300-second and
strict-ID library admission retain their existing contracts.

No dependency, public media encoding, Android permission/service declaration,
real-device deployment, persistent background service or system security setting
is added. A source edit or software fixture is not device interoperability proof.

## Explicit discovery and dispatch eligibility

Catalog inspection/export accepts only the exact Android Speaker/Sink stereo
format with empty parameters, Interactive QoS and the existing AdapterManaged
contract. Discovery is a candidate hint requiring local selection, not consent.
The new Linux Source capability has the exact `Custom("system_audio_capture")`
class (not Speaker), and publishes the exact metadata pair
`capyio.secure_stereo.bilateral_consent: v1`. This authenticated catalog marker
selects the bilateral dispatch contract; it grants no resource authority.
Unmarked legacy routes are not silently migrated. Marked candidates still need
the exact Linux Source, selected Android secure speaker, dedicated Session and
fresh independent approvals. A malformed marker cannot select a downgrade.

## Fresh user intent and ordered handoff

The launcher owns bounded inherited FIFO control and a dedicated authenticated
Session. It proposes Pending only, emits a fresh request/Route identity and
requires a matching explicit response after the prompt; queued/partial preinput,
wrong request/Route and input EOF fail closed. The console additionally displays
and matches the exact paired peer. Linux approval creates the original finite
Authorized Draft; Android approval must be fresh after its decoder-minted
receipt. Neither pairing, publication, an early click nor a prior invocation
supplies that approval.

Android intersects its bound and sends immutable Prepared before Source Acquire.
The launcher consumes the opaque reverse receipt and transfers the same Runtime,
link, wall high-water and original monotonic cap into the receiver-assigned
Source owner. It does not acquire a Source lease or enter Starting using the old
UDP/mono launcher path. Crossed claims and a single Start precede media setup.
Only the receiver's authenticated offer chooses StreamId; capture starts only
after actual pinned media TLS Bound. No timer refresh, renewal, reconnect,
plaintext fallback or automatic retry is introduced.

## Diagnostics and terminal ownership

Stereo PCM is 1920 bytes per block; the two canonical encoded fragments occupy
2128 bytes per media record. Counters distinguish captured PCM, sent packets and
encoded record bytes. Terminal reasons use the actual Source owner cause,
including expiry, clock regression and catalog/session changes. These are not TLS overhead counters, receiver acceptance,
AudioTrack rendering, Route Active or audibility. The console must not apply the
mono 960-byte arithmetic to this path.

Stop, denial, expiry, control/terminal EOF and malformed operations cancel the
owned attempt. Authorized failures retain B2 target-only Failed + Revoked
retirement and shared revocation. Owned workers and claims are released without
modifying unrelated Sessions. Unknown cleanup is an error, not clean completion.

## Validation and remaining proof boundary

Run the host/console unit suites, new `linux_secure_stereo_entry` Node subprocess
suite with paired-store enabled, and the retained control-link, Runtime, Android,
Linux-audio and exact-ID/receiver-assigned Stage A tests serially. New subprocess
fixtures use a private synthetic Secret Service and actual pinned loopback TLS,
not copied grants or stored production credentials. Test fresh approval ordering,
wrong/replayed identities, denial, finite expiry, EOF, crossed claims/Start and
no helper before media binding. Production dispatch tests must distinguish the
marker from legacy and enforce original receipt/selection/generation authority.

Private PipeWire/PTY waveform, left/right/silence, partial ACK and backpressure
acceptance require their own executed evidence. Missing system dependencies,
Android-target compile or real-device checks must be reported separately; they
cannot be inferred from portable tests. No public feature publication is implied
by this candidate ADR.
