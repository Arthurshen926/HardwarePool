# ADR 1524: Linux-led Android audio negotiation

Status: implemented; independent review and exact-head hosted validation pending.
Date: 2026-10-20

## Problem found by compatibility audit

ADR 1522 executable tests used an authenticated synthetic Rust peer, not the
Android coordinator. Passing those tests did not prove Android interoperability.
The Linux command sent a new Starting/Authorized Route with a media offer, while
real peer admission requires a new Draft/Pending proposal. It also omitted remote
Source lease acquisition and the explicit Start command that starts Android
capture. Android sends ordinary catalog and consent status messages that the
initial Linux owner rejected. Android's normal audio consent expiry can be eight
hours, incompatible with Linux's local 1–300 second approval and exact reciprocal
grant matching. PCM/profile/stream binding was not the mismatch.

## Decision: one Linux-led Route, existing protocol

After authenticated catalog exchange and exact local launcher approval, Linux
publishes the still unauthorized Draft proposal. Android independently obtains
its existing local resource consent. Linux waits for that finite grant; it does
not grant permission on Android's behalf. It selects the earlier of the Android
grant and its own approval-time-plus-duration deadline, authorizes only its own
Draft, and sends that finite status. Android may only shorten its already
consented Draft grant to the same deadline. Linux waits for the exact matching
acknowledgement, acquires the remote microphone lease using the existing request
protocol, then acquires its local Sink lease and prepares/starts its local Route.

Only then does Linux send its dynamic Sink offer. After the exact reciprocal
Source offer passes existing checks and the actual local worker opens, Linux sends
one Start command to Android. Ordinary status/catalog messages are not readiness
or authorization; they are validated/applied through the existing authenticated
link. Changed bindings, denial, terminal peer state, expiry and lease failure
remain fail-closed. Duplicate offers do not reopen the worker or resend Start.
Android local Stop also invalidates an Authorized Draft with a prepared endpoint
and releases its lease/job, so a delayed Start cannot revive cancelled capture.
The Java catalog reports Busy during its foreground audio lifecycle. Linux accepts
that state only for its exact remote Source after reciprocal offer/worker/Start;
initial Busy, permission-required or offline metadata never admits playback.

The peer negotiation has an absolute 30-second ceiling, further bounded by local
approval expiry, and polls launcher Stop/status between bounded control steps.
Local approval lifetime begins before peer negotiation, not after it. The lease
request reserves one second of transit headroom, as the Windows path does; the
protocol also requires a minimum one-second lease. Thus very short configured
lifetimes or delayed consent can expire before media starts. This is a reported
failure, never a reason to extend approval. Clock skew/slow transit can likewise
fail closed. No auto-renewal or reconnect is introduced.

## Constrained authority API

`Route::restrict_draft_authorization` and its NodeRuntime wrapper allow an
existing finite, unexpired Authorized Draft to keep or shorten its expiry.
Pending/revoked/unbounded/expired grants, later lifecycle states and extensions
are rejected. The Runtime wrapper rejects any retained Route lease, including
expired-but-not-reconciled leases. Equal expiry is idempotent and emits no event;
actual reduction emits RouteChanged. Existing authorize and renewal APIs remain
unchanged. This is a local-owner operation, not a peer authority token.

Android applies it only after matching the exact authenticated Session/Route,
endpoints, profile/backend and Draft state; no media job or offer may accompany
this policy step. It does not replace the local consent gate or platform audio
permission. The actual Android audio policy is extracted from target-only JNI
into a host-testable module, and JNI calls that same implementation. The Android
initiator's Windows-specific peer selector is deliberately unchanged; this ADR
covers only Linux-initiated playback.

## Compatibility and evidence boundaries

No wire schema, TLS pin/binding, UDP format, Android permission/foreground-service
manifest, credentials, installation, or Windows platform workflow is changed.
Existing blocking link send methods remain; additive deadline-aware lease and
Start methods support the bounded Linux owner. Linux still plays through pw-cat,
not a virtual microphone. UDP remains unauthenticated and unencrypted.

Pure tests cover constrained authority, exact ordinary status/catalog handling,
and finite negotiation budgets. Hosted command tests use the repository Android
catalog and actual AndroidCatalogRuntimeV1 coordinator/consent/lease methods,
with ephemeral test identities and explicit test-side user consent. They verify
proposal/consent/narrowing/lease ordering, denial and cancellation; they do not
pretend to execute Java audio hardware, launch PipeWire or validate a physical
microphone. Separate worker tests check Start-after-worker-open and duplicate
suppression. Real Android-target compilation, Java/device capture, PipeWire sound
and the [device acceptance checklist](../reports/LINUX_PLAYBACK_DEVICE_ACCEPTANCE.md)
remain separate evidence gates, not consequences of a passing Rust-hosted test.
