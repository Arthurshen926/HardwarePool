# CapyIO UX Model

## Quick Actions

The default view starts with outcomes:

```text
Use phone as microphone
Use tablet as speaker
Use phone as webcam
Use phone as controller
Use tablet as second display
Share keyboard and touchpad
Record sensor suite
```

A Quick Action is a versioned Route Template. It resolves Source/Sink Ports,
selects an Adapter/backend, chooses format/QoS, requests authorization, creates
required Projection Capabilities and starts Routes. Foundation actions operate
only on deterministic fixtures and say so visibly.

Internal terms such as `AdapterManaged` are not shown in the simple mode unless
needed to explain a limitation or error.

## Workspace / Lab

Workspace provides initial navigation for:

- Overview / Connections;
- Nodes;
- Capabilities and Ports;
- Routes;
- Adapters;
- Problems and Logs;
- Panels;
- Recorder and Playback.

The first Route Builder uses cards, lists and selectors with compatible choices
filtered or explained. A node-graph editor and dynamic Panel marketplace are not
part of the foundation.

## State and failure

- Route controls are independent and keyboard accessible.
- Pending/busy state is scoped to the affected Route.
- Problems show user action, stable code and sanitized detail.
- Offline/failed Adapters do not blank the whole workspace.
- Mock state, authorization and metrics are labeled simulated.

## Platform truthfulness

UI distinguishes system Projection, system route/injection, standard API and
Panel/Recorder fallbacks. It never claims a system-level device when only an
application-local path exists.

The initial Audio Share remote-speaker action is labeled as system-audio
mirroring. It captures an existing Windows render endpoint, so the desktop may
continue to play locally; it is not presented as a CapyIO-created virtual
speaker or exclusive output route.

The desktop card can rescan current Windows playback endpoints and select one
by display name while the Route is inactive. Selection applies to the current
desktop process only; users must stop the mirror before changing endpoints.
Raw platform endpoint identifiers are never displayed or accepted by the UI.

The final Windows workflow targets a separate system render device named
`CapyIO Speaker`. Applications may choose it without changing where unrelated
desktop audio plays. Until Gate 7B has isolated-VM enumeration and end-to-end
evidence, the existing Quick Action continues to label itself as mirror mode
and must not imply that the virtual endpoint is already installed.

## Runtime ownership

Closing or refreshing UI does not define the lifecycle of an active Node or
mobile service. UI reads snapshots and submits intent through a narrow local API.
