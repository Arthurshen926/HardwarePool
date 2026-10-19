# ADR 1464: Authenticated peers may reconnect after closed Sessions

- Status: Accepted admission primitive; liveness detection pending
- Date: 2026-09-18

## Context

First-connection admission deliberately rejected any peer already present in a
Runtime. After an explicit disconnect, `close_session` correctly retained the
peer catalog as offline history, so a later authenticated connection was also
rejected as if it were a concurrent duplicate.

## Decision

`CatalogControlLink` adds dialed and accepted admission methods which distinguish
three cases transactionally:

1. an absent peer uses the existing provisional new-peer path;
2. a known peer whose earlier Sessions are all `Closed` is marked online and a
   fresh authenticated Session ID is opened;
3. a known peer with any non-closed Session is rejected as a duplicate.

The old new-peer-only methods remain for callers and tests that require that
stricter assertion. Windows outbound connection and Android accepted catalog
exchange use the reconnect-capable methods. The newly received catalog remains
authoritative after reconnect and replaces retained offline inventory through
the existing admission path.

## Consequences

- Explicitly closed peers can reconnect without deleting paired metadata or
  restarting either application.
- Concurrent duplicate Sessions remain forbidden.
- Passive half-open detection is not solved: a host must first observe or
  explicitly close a dead transport before reconnect admission can succeed.
