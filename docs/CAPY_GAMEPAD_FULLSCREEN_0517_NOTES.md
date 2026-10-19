# Android controller fullscreen — 0.5.17-dev

The shared controller surface now has a separate fullscreen presentation on
Android, entered from the controller resource details or controller settings.
It hides catalog, telemetry and configuration, keeps a Return to settings action,
and uses viewport-relative controls in landscape and portrait layouts. It does
not start hardware or change pairing. Disconnected input remains disabled.

The API reuses the existing Android input-surface immersive presentation hook
(historically named setTouchpadImmersive) for system bars and keep-awake; no new
permissions or foreground-service declarations were added. Leaving the surface,
window blur and document hiding release UI controls. Fullscreen polling only
reads controller state rather than unrelated audio/video/IMU panels.

Changed areas: App.vue, GamepadSurface.vue, styles.css, API contract and its
Android/Tauri/mock implementations, Android version metadata.

Validation on 2026-09-28:
- Shared Vue typecheck/Vite build and offline Android assembleDebug passed.
- git diff --check passed.
- Authorized in-place install succeeded on the Y700, preserving app data.
- Normal UI navigation reached the full viewport controller surface; screenshot
  confirmed hidden Android status/navigation bars, visible controls without
  scrolling, explicit disconnected state and Return to settings.
- Return to settings reached the controller configuration page.
- First install exposed a Vue initialization-order error; fixed before the
  successful UI check and replaced the APK again.

This check intentionally did not start a new controller stream. The user's
responsive Windows browser gamepad test predates this UI update. Active input
release, concurrent fingers, portrait layout and reconnect need a further
physical pass; this is not a full controller acceptance report. VIIPER automatic
startup and the unified Windows start/stop workflow remain separate open work.
