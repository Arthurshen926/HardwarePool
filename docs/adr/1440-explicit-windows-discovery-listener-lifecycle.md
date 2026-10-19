# ADR 1440: Explicit Windows discovery listener lifecycle

- Status: Accepted
- Date: 2026-09-18

## Context

The shared UDP discovery host had no application owner. Blocking receive while
holding the Node Runtime lock would stall unrelated UI and hardware operations,
and advertising a control port before an authenticated server exists would
publish a false endpoint.

## Decision

Windows Desktop owns a separate discovery worker and control-plane mutex. The
Settings page starts it only after the user supplies one reviewed local IPv4
interface and can stop it independently. Its 250 ms receive poll never holds the
Node Runtime lock. Accepted datagrams enter the bounded shared ingress; idle
polls expire stale candidates. Stop, application shutdown and worker failure
clear the listening state, and Stop joins the worker.

This receiver does not announce the Node because no application-owned mTLS
control server port is ready. It does not pair a candidate, create a Session,
admit a catalog or start a Route. Reset is rejected while discovery is active so
the ingress cannot retain the previous local Node identity.

## Consequences

The ingress clock is owned by the control-plane state and survives listener
restarts. Expiry runs after each receive, including under continuous traffic.
Completed workers are reaped before retry. Settings and remote-resource pages
refresh their snapshot every two seconds with one outstanding catalog request.
This is receive-only discovery; no end-to-end pairing claim follows from it.

- Windows now has a real discovery receive lifecycle reflected by the shared
  UI, without blocking Runtime operations.
- Interface enumeration/default selection, node advertisement, Android
  multicast lifecycle, pairing and authenticated catalog exchange remain
  separate acceptance gates.
