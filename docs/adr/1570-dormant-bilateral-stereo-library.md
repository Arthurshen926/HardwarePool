# ADR 1570: Dormant bilateral secure stereo library handshake

Status: Candidate

## Scope and activation boundary

B2 adds opt-in portable Rust library supervisors. It does not switch existing
Android JNI/audio dispatch or implement a Node, console, CLI or Linux launcher.
The mono `platform/linux/capyio-host/src/launch.rs` is not a stereo consumer.
Existing strict StreamId admission and mono/legacy paths retain their contracts.
No wire format, production dependency, permission, service, credential or hardware
change is included. Library tests are not physical AudioTrack or audibility proof.

## Independent approvals and ordered control

Each attempt owns one exact Route on a dedicated authenticated control Session.
Other Sessions may exist; closing this dedicated link reconciles its Session.
Both TLS control roles and both proposal origins are supported. A Source-origin
Pending proposal is imported/staged by Android; a receiver-origin Pending is
imported by Linux. Neither import grants local resource authorization.

Linux explicit local source approval captures a finite wall expiry and original
monotonic deadline. It sends Authorized Draft first. Android requires a fresh
independent approval after the decoder-minted B1 receipt; early approval is not
queued. Android intersects its local finite limit with the original peer bound
before stamp creation and immutable Prepared. It sends Prepared before its
Source Acquire. Linux does not echo Prepared: that would conflict with Android's
retained Authorized Draft evidence.

Linux consumes a separate opaque authenticated reverse Prepared receipt, checks
full exact authority and narrows its own Draft once before local preparation.
The peer's Prepared is never edited to manufacture agreement. Draft and Prepared
remain epoch zero; only the once-only Start advances each peer to Starting epoch
one. Prepared is neither Active nor evidence of media.

The same Linux Runtime, link, original approval cap and zero local Source lease
move into receiver-assigned source ownership. The queued Android Source Acquire
is left for that owner; it emits Sink Acquire and completes crossed claims before
one Start. The receiver chooses its StreamId only through the existing
receiver-assigned authenticated offer path.

## Provenance, clocks and revocation

A reverse receipt is decoder-minted, non-Clone, private-connection/certificate
bound and one-use. Its bounded cache retains tombstones and original wall and
monotonic anchors. Duplicate receipt/status cannot refresh authority. Caller-
constructed status DTOs do not establish provenance. Later legitimate Starting
and offer traffic is delivered to the lifecycle owner, not rejected by a
prelaunch receipt cache; consumed receipt liveness is not perpetual media consent.

Original approval, request, receipt and operation deadlines survive all waiting
and handoff. The source's B2-only owner context carries observed wall high-water
before any initial Acquire. Android retains a shared high-water across control
and media callbacks. Clock rollback and expiry fail closed.

Android's independently shared private revocation gate survives stamp consumption,
negotiation and media creation. Cloned host invalidation and registered selection
changes revoke this gate before catalog mutation. Existing job admission and
Runtime callbacks enforce it at subsequent authority-sensitive boundaries. A
prelaunch receipt is not reused as this persistent revocation capability.

## Terminal cleanup and ownership

An authorized partial failure requires actual Runtime target retirement to
Failed + Revoked, not only stamp consumption, a stopped worker or recoverable
Offline. Source construction has a B2-only ownership guard and returns the
retained authoritative Runtime plus cleanup errors on failure. B2 owner terminal
cleanup retires the target before dedicated-session reconciliation. Existing
constructors do not acquire this changed cleanup policy implicitly.

Android supervisor failures retire the target and invalidate shared gates before
reconciliation. Catalog/control retirement hooks affect explicitly registered
B2 Routes only. Other-session Routes, claims and pending operations must remain
unchanged by target retirement. No renewal, reconnect, reapproval of retired
lineage, transport downgrade or false Active transition is introduced.

## Validation contract

Real pinned loopback TLS tests must call the exported portable library APIs with
independent runtimes and independent approval actions. Required cases cover both
proposal origins and TLS roles, exact Prepared equality, zero Source lease at
handoff, crossed Acquire/Start, early-click rejection, shorter grants on either
side, original deadline exhaustion, constructor retained-Runtime failures,
post-stamp and post-finish clone invalidation, and terminal owner cleanup.

Receipt tests additionally cover forged DTOs, equal-binding wrong instances,
replay, conflict, overflow, malformed input and legitimate posthandoff traffic.
Boundary tests cover wall high-water, mutation/failure retirement and other-session
preservation. Retain B1, strict-ID, mono and legacy regression checks. Report
executed commands/results separately from unavailable workspace or device checks;
passing library tests does not activate a production workflow.
