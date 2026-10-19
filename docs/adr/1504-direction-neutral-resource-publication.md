# ADR 1504: Direction-neutral resource publication

Status: Accepted for Android speaker publication slice; general cross-platform selection remains proposed.

## Implemented slice

AndroidSharingSelection accepts the local speaker in addition to Source-bearing
capabilities. It strips speaker selection before the existing Source selector,
then omits an unselected speaker from the Android publication catalog supplied
to Rust. The shared Source-only encoder is unchanged: it cannot reintroduce a
Sink omitted by the host. Haptics remains implicitly available for compound
controller feedback. The full local UI inventory retains the speaker checkbox.
One-time persisted migration selects the formerly automatic speaker; later
explicit deselection is retained. Publication changes do not start playback,
grant permission or automatically stop an existing Route.

## Problem

The user expects a phone speaker to have the same resource-publication checkbox
as its microphone and camera. Current ARCHITECTURE.md explicitly restricts the
selector to Source-bearing capabilities and automatically retains Sink/Control
capabilities. AndroidSharingSelection and CapabilityPublicationProjection enforce
that rule. Adding a working speaker checkbox would change the normative contract;
adding a UI-only checkbox would falsely imply that the resource can be hidden.

## Proposed decision

Separate resource publication from Port direction. A selected complete capability
may expose Source, Sink or Control ports without giving a Node a global role.
Keep publication independent from hardware start, active Route stop, and consent.
Apply the rule consistently to Android and Windows catalog projection, persisted
selection and peer catalog updates. Migrate existing speakers to selected to
preserve current behavior, but honor later explicit deselection after restart.
Compound gamepad feedback remains governed by its documented compound-resource
policy, rather than implicitly disappearing with a Source-only selector change.

## Required acceptance before enabling the checkbox

- Deselecting the speaker removes it from the next authenticated peer catalog.
- Selecting does not start playback or grant consent.
- Saved selection survives restart; migration does not re-enable opt-outs.
- Existing active-route behavior is explicit and unchanged by publication alone.
- Both platform UIs and shared publication validation use the same semantics.

Physical peer-directory refresh and restart persistence still require device
verification. Generalizing Windows Sink selection is outside this slice.
