# CAPY-PTP-003U — Android OEM multitouch conflict diagnostics

Status: completed

## Goal

Make an OEM system-gesture interception visible and actionable when Android
cancels a three-/four-contact stream before CapyIO can forward it to Windows.

## Acceptance evidence

- `ACTION_CANCEL` after at least three contacts increments the visible bounded
  probable-system-interception counter and emits a diagnostic log.
- The first conflict explains the phone-side boundary and offers a
  user-initiated vivo Super Screenshot settings route with generic fallback.
- The app changes no setting and adds no permission; v0.8 builds and lints with
  only the already approved `INTERNET` permission.
- After the user disabled the conflicting OEM gestures, the accepted physical
  run submitted all 2,050 frames, repeatedly reached four contacts and closed
  cleanly. The user confirmed working three-/four-finger Windows effects.

## Scope boundary

Android system gesture exclusion rectangles cover navigation-edge gestures;
they do not grant authority over OEM full-screen monitors. CapyIO reports the
conflict and routes the user to settings but never writes hidden settings or
fabricates missing contacts.
