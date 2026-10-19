# ADR 1438: Report control-plane readiness in the shared UI

- Status: Accepted
- Date: 2026-09-18

## Context

CapyIO now has bounded discovery, paired-peer classification, mutually
authenticated control transport and owner-authoritative lease foundations.
Neither the Windows nor Android application host currently starts all of those
components. The shared UI previously collapsed that distinction into the vague
text “not integrated”, which could also be misread as either no implementation
or an already usable automatic connection.

## Decision

`UiSnapshot` schema 5 contains one required schema-v1 `controlPlane` record on
Windows, Android and Browser Mock hosts. It reports discovery-listener, pairing,
authenticated-transport and catalog-exchange state separately, with bounded
candidate, paired-peer and authenticated-connection counts. Browser Mock marks
the record simulated. Production hosts report stopped, unconfigured or
disconnected until their application process actually owns the corresponding
component.

This record is observation only. It does not open a socket, enroll a credential,
authorize a Session, create a Route or start a data-plane worker. The UI must not
infer availability or authorization from a Node being online.

## Consequences

- Both platform shells expose the same navigation and readiness vocabulary.
- Completing a foundation crate cannot be presented as a live application
  connection.
- Later host integration may replace the static zero states with bounded owner
snapshots without changing the initial control-plane DTO schema.

The first Windows integration stores the bounded discovery and paired-peer
directories inside the process-owned Node state and projects their actual
counts. This intentionally stops before socket ownership or credential-store
access.
