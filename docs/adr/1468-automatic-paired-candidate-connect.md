# ADR 1468: Automatic paired-candidate connection

- Status: Accepted Windows shared-UI composition
- Date: 2026-09-18

## Context

Android advertised an ephemeral control endpoint and Windows could discover and
connect it, but every application start still required choosing a local network,
starting discovery and pressing Connect. This preserved security but did not
meet the intended one-device-connection interaction.

## Decision

The shared UI enables automatic trusted-peer connection by default on the
Windows host. It persists only the chosen local interface address and a boolean
preference. Native interface inventory must still contain that address before
discovery starts. If exactly one address exists it may be selected automatically;
multiple unknown networks still require an explicit choice.

After each bounded candidate refresh, the UI selects at most one candidate that
the native pairing directory classifies as active paired. Connected, unpaired,
revoked and manually disconnected peers are excluded. Failed attempts have a
ten-second per-peer cooldown. Manual disconnect suppresses reconnection for the
rest of the UI session, and the setting can disable automation without stopping
an explicitly running listener.

## Consequences

- A previously paired Android device normally needs no IP or ephemeral port
  entry and reconnects through discovery.
- Browser/Android hosts keep the same UI source but do not invoke the Windows
  discovery/dial API.
- The preference is not authority: native pairing classification, leaf pinning
  and mTLS remain mandatory.
- First-run pairing and physical network qualification remain open.
