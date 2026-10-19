# ADR 1478: Explicit Android background node waiting

- Status: Accepted for the integrated alpha
- Date: 2026-09-19

## Context

ADR 1474 keeps the Android control listener alive after leaving the UI only
when at least one Source is published. A paired but unshared Node consequently
disappears from the network on Activity unbind. Publishing hardware solely to
retain connectivity couples two independent user choices.

## Decision

The Android Settings page offers an explicit, initially off, persistent
"keep node online in background" choice. It starts the existing non-exported
Node Service with its already declared `connectedDevice` foreground type and
visible notification. The Service accepts the choice only if its paired
control listener exists and foreground promotion succeeds. Closing the UI
does not stop this owner. Reopening the UI can restore the stored choice;
`START_NOT_STICKY` means process death or device boot does not silently restart
it. Turning it off relinquishes only this waiting owner: published Sources and
active hardware retain their independent lifecycle. Stop All clears the choice.

This switch authorizes no peer, Route, capture or input. Existing pinned mTLS,
per-resource consent and ordinary OS permission gates remain mandatory. It
adds no permission, manifest declaration, dependency or boot receiver.

## Limits and verification

Android is still the control listener/discovery announcer and Windows the
principal dialer. This is not a symmetric inbound/outbound connection design
or first-time pairing UX. An OEM may stop even a visible foreground Service;
automatic recovery from that event is not promised.

Pure ownership tests cover the independent wait owner. On the authorized Y700,
verify same PID/listening port after Home, `connectedDevice` foreground status,
pinned mTLS/catalog from the paired Windows account, and release after disabling
the switch. The UI displays the choice without implying that a data Route is
active.
