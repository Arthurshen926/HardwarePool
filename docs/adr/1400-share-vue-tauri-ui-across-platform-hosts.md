# ADR 1400: Share one Vue/Tauri presentation across platform-owned Node hosts

Status: accepted

## Context

CapyIO now has non-display vertical prototypes for bidirectional audio, camera,
IMU/gamepad and Precision Touchpad. The Windows control surface already uses
Vue 3, TypeScript and Tauri 2, while Android functionality is split between the
audio Node application and separate camera, controller and touchpad lab apps.
Copying each screen into a native Android Activity would create two product
interfaces and two DTO contracts. Moving hardware callbacks, permission logic
or media workers into the WebView would instead violate the Node lifecycle and
real-time boundaries.

EasyTier is a useful implementation reference rather than a dependency: its
official GUI documentation describes one Vue/TypeScript/Tauri GUI built for
desktop and Android. CapyIO has different hardware, permission and lifecycle
requirements, so only the cross-platform presentation pattern is adopted.

## Decision

1. Keep one Vue 3 and TypeScript presentation source. The existing
   `apps/desktop/src` tree is the initial shared UI source; its historical path
   does not define a Windows-only product boundary.
2. Use Tauri 2 as the thin platform shell on both Windows and Android. Each
   shell implements the same narrow `CapyIOApi` DTO contract. Platform services
   and Adapters remain native Rust/Kotlin/Java boundaries and are never moved
   into UI components.
3. The Node Runtime or platform-managed service owns hardware enumeration,
   permissions and long-running Routes. Closing an Activity or desktop window
   must not define Node lifetime.
4. `UiSnapshot` schema 5 projects a Runtime capability catalog for both the
   local Node and connected peers. It includes Capability availability,
   permission requirement, typed Source/Sink/Control Ports and a schema-v1
   control-plane readiness record. Fixture-backed catalogs and selections
   remain visibly marked simulated.
5. The UI has two task areas: **local capabilities** and **peer capabilities**.
   These are not global sender/receiver roles. A local Source may be selected
   for publication, a local Sink remains a possible Route target, and the same
   rules apply symmetrically to every peer.
6. Selecting a local Source records publication intent only. It does not grant
   a Session, authorize a Route, request an Android permission, start a
   foreground service or start a data-plane worker. Those remain separate
   explicit operations.
7. Android integration will replace the current native screen with the shared
   presentation only after a host/plugin boundary can supply schema 4 without
   changing the proven audio service lifecycle. Camera permission and any new
   foreground-service declaration require a separately approved change.

## Consequences

- Windows and Android can converge on one interaction model and visual system
  while retaining different platform implementations.
- Browser Mock, Windows Tauri and future Android Tauri builds must evolve the
  DTO together; platform-only fields stay behind host commands.
- The current schema-4 Tauri catalog is still deterministic fixture evidence,
  not proof of Windows or Android hardware enumeration.
- Camera, controller and touchpad lab applications remain separately buildable
  until their code is extracted behind the unified Android host. They are not
  a second long-term product UI.
- A later Route Builder can consume the same catalog, but publication selection
  must not be presented as peer authorization.

## References

- EasyTier GUI: <https://github.com/EasyTier/EasyTier/blob/main/easytier-gui/README.md>
- EasyTier contribution/build guide: <https://github.com/EasyTier/EasyTier/blob/main/CONTRIBUTING.md>
- ADR 0017: Tauri control-plane security
- `docs/ARCHITECTURE.md`
- `docs/SECURITY_MODEL.md`
