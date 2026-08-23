# GUI-specific Agent Rules

- Vue components consume `CapyIOApi`; they do not call platform APIs directly.
- Browser Mock and Tauri backend must implement the same TypeScript DTO contract.
- Mock metrics and authorization must remain visibly labeled as simulated.
- Do not add shell, arbitrary filesystem, remote-content, updater, or broad Tauri plugin permissions without explicit security review.
- UI actions are scoped to one Route and never implicitly toggle another.
- Quick Actions hide unnecessary Port/Adapter terms; Workspace may expose them with explanations.
- Accessibility, keyboard focus and narrow-screen behavior are acceptance requirements for UI changes.
