# ADR 1420: Bind the elevated camera Broker to the requesting user

Status: accepted

Date: 2026-09-16

## Context

The first projection-scoped Broker used the named-pipe object-owner principal.
That works when UAC retains the same account, but it does not admit the ordinary
desktop when Windows `runas` crosses to a separate administrator account. A
physical gate reached the elevated Broker and created its pipe, while the
ordinary control client correctly received no access.

Granting all interactive users would make the gate work but would broaden the
local authority beyond the initiating desktop. Running all of Tauri elevated
would restore the privilege boundary ADR 1419 is intended to remove.

## Decision

Before requesting UAC, the ordinary desktop reads the user SID from its own
process token. It launches only its current executable with the fixed
`--camera-broker-host` mode and exactly that SID. The elevated entry point
accepts no other argument, requires an elevated token and asks Windows to parse
the supplied SID before using it in the named-pipe DACL.

The pipe grants access only to LocalSystem, administrators and that exact
requesting-user SID, and continues to reject remote clients. The SID is bounded
to 184 text characters, must use the `S-1-` form and cannot be supplied by the
WebView. No executable, path, endpoint, mapping name or camera identity crosses
this handoff.

## Evidence

Unit tests reject malformed and DACL-injection-shaped SID text, assert the
requester-specific SDDL and perform the real named-pipe lifecycle round trip.
On the authorized Windows lab host, an ordinary process under user SID ending
in `-1004` launched the fixed Broker through cross-account UAC. The elevated
Broker ran under the administrator SID ending in `-1001`; the original ordinary
client then successfully obtained Stopped, sent Start and observed Active with
generation 1. The fixed session host and AVC receiver were both alive and the
expected ADB reverse existed. Stop returned Stopped and removed all three
processes and the reverse mapping.

The phone was connected at the newly supplied wireless-debug endpoint, but its
keyguard was locked during the first gate. No authentication bypass was
attempted, so that gate proved ordinary-control-to-elevated-Broker lifecycle and
cleanup, not live frames.

After the phone was manually unlocked, the unified Android App exposed its
shared Quick Actions home and `Phone camera` entered Active. The Broker's fixed
ADB reverse, session host and receiver delivered a 1,382,400-byte NV12 sample to
an independent Media Foundation consumer running as the Broker account. Android
camera clients, the reverse and all three Windows processes were empty after
Stop. A consumer launched directly by Codex runs as the distinct restricted
`CodexSandboxOffline` account, while the interactive Explorer and Broker both
run as `local-user`; its access result is therefore not acceptance evidence for the
ordinary `local-user` desktop. At that stage the final Explorer-launched
ordinary-user consumer gate still required a separately confirmed interactive
UAC run.

That final gate passed on 2026-09-17. The Broker was launched for the actual
interactive `local-user` SID; an ordinary Explorer-launched controller observed
generation 1 Active while the unified Android App held Camera ID 0 and the
fixed `tcp:38173` reverse. An independent Media Foundation consumer launched
from the same ordinary Explorer context read one 1,382,400-byte NV12 sample
with 333,333-tick duration and delta (`first_luma=235`). Ordinary-user Stop
returned Stopped, and cleanup left no Android camera client, ADB reverse,
desktop Broker, session host or AVC receiver. No captured payload was retained.
