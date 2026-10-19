# ADR 1486: Usable touchpad sessions with renewable short leases

Status: accepted for the user-requested integrated delivery work

Date: 2026-09-26

## Context

ADR 1472 granted five minutes of touchpad use and acquired a non-renewing
lease. This interrupts ordinary use and makes a healthy input session appear
to fail. The owner still needs explicit, finite and revocable consent.

## Decision

- The Android owner's "Allow this use" grants only the exact Route and Ready
  Session for at most eight hours. The UI states this limit. Denial, a new
  Route, reconnection or application restart never inherits this grant.
- This supersedes only ADR 1472's five-minute authorization duration. Pending
  requests still expire after 60 seconds and no persistent trust is inferred.
- Port leases retain the existing protocol limit of five minutes. The Windows
  control owner renews the active touchpad Source lease every 60 seconds and
  then refreshes its local Sink lease, each bounded by the original grant.
  The data worker never renews its own authority. A missing/expired local
  claim, denied renewal or terminal control error stops input and releases it.
- The data TLS binding retains the original authorization expiry, Route,
  Session, Stream and epoch; lease renewal cannot mutate that binding or extend
  authorization. An expired grant requires a new explicit request and consent.
- Correlated lease replies allow bounded intervening catalogs and heartbeat
  requests on the authenticated control owner. Wrong replies fail closed.
- Stop, publication loss, Session loss and data failure retain existing cleanup
  semantics. No new permission, dependency, wire field or driver operation is
  introduced.

## Verification

Deterministic tests cover finite consent, renewals beyond the former five-minute
boundary, final grant expiry, missing/expired claims and unrelated Route
isolation. Real TLS tests cover an interleaved catalog before a lease reply.
A fixed-duration physical soak longer than five minutes must use the existing
paired identity and visible owner consent, retain advancing input/heartbeat
evidence and explicitly stop/clean up. Compilation alone is not soak evidence.
