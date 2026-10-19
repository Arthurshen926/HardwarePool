# ADR 1443: Read-only Windows discovery interface inventory

- Status: Accepted
- Date: 2026-09-18

## Context

ADR 1440 required users to type a local IPv4 address. This is easy to confuse
with the peer or Android debugging endpoint and offers no stale-address check.

## Decision

The Desktop host exposes one parameter-free `get_discovery_interfaces` command
through CapyIOApi. The existing windows-sys dependency gains only the IP Helper
feature; no new package is introduced. `GetIpAddrTable` runs locally without
opening sockets, launching commands, modifying networking or exposing MAC,
DNS, gateway, credential or unrelated adapter metadata.

The native buffer is capped at 1 MiB with three resize attempts and 4096 input
rows; the DTO is capped at 64 unique address/index entries. Disconnected,
deleted, transient, loopback, unspecified, multicast and reserved addresses
are excluded. Start re-enumerates before binding to reject a vanished address.
This is address availability, not proof of reachability or multicast support.

Settings reads the inventory when opened, defaults only an unambiguous address,
preserves an available explicit selection and supports manual refresh. Multiple
networks are never silently ranked. Browser Mock and Android return no entries
and expose no Windows discovery control. The shared parser rejects malformed,
duplicate, unknown-field and oversized inventories.

## Consequences and acceptance

Normal users no longer need to type the local discovery address. Interface
friendly names, automatic selection between multiple networks, Android discovery,
announcements, enrollment and authenticated catalog exchange are not delivered
by this slice. No driver, firewall, APK permission or credential policy changes.

Rust tests exercise address/state filtering and the real read-only Windows IP
table. `node --test scripts/discovery_interfaces_ui.test.mjs` covers validation,
single-network defaults, ambiguous networks and selection invalidation. Build
and contract results do not establish two-device discovery or hardware use.
