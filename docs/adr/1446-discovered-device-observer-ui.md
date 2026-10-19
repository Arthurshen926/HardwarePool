# ADR 1446: Discovered device observer in the shared UI

- Status: Accepted
- Date: 2026-09-18

## Context

The Windows discovery worker exposed only a candidate count. Users could not
see which endpoints were observed, and stopping reception left expired entries
in memory until another worker was started.

## Decision

A parameter-free `get_discovery_candidates` command projects at most 64 entries
from the existing bounded ingress. Observation first expires stale candidates
using the control-plane monotonic clock, even after reception stops. Each entry
contains only Node ID, source-derived IP, proposed control port and observation
age. No credential, nonce, trust assertion or resource catalog is exposed.

The Remote resources page renders candidates separately from authenticated
resource catalogs. Every card is explicitly unverified. It offers no Pair,
Connect or hardware action until enrollment is implemented. The existing
two-second single-flight refresh reads candidates before the summary snapshot,
and failed candidate reads clear the visible list rather than retain stale
observations. A closed DTO parser bounds records, IP syntax, IDs, ports and age.

Android and Browser Mock return an empty list and do not pretend to have a
discovery implementation. The UI explains that the current Android application
does not broadcast candidates yet. Source-derived addresses are not DNS names,
trusted peers or grants; no extra socket, service or permission is added.

## Acceptance

The native regression proves exact fields, age, post-stop expiry and no pairing
or connection side effects. Shared UI tests reject malformed, repeated, oversized
and stale records, extra trust fields and noncanonical addresses. The responsive
list permits UUID/address wrapping and retains keyboard navigation to Settings.
Build and parser tests do not establish visual or physical discovery acceptance.

First-run pairing, secure credential persistence, Android discovery and actual
application catalog exchange remain incomplete. This is an observer entrypoint,
not the product's connection workflow.
