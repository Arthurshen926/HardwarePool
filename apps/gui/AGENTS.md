# GUI-specific Agent Rules

- Vue components consume `HardwarePoolApi`; they do not call platform APIs directly.
- Browser Mock and Tauri backend must implement the same TypeScript DTO contract.
- Mock metrics and authorization must remain visibly labeled as simulated.
- Do not add shell, arbitrary filesystem, remote-content, updater, or broad Tauri plugin permissions without explicit security review.
- UI actions for microphone and speaker remain independent.
- Accessibility, keyboard focus and narrow-screen behavior are acceptance requirements for UI changes.
