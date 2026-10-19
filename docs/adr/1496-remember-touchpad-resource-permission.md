# ADR 1496: Remember touchpad permission for a trusted peer

Date: 2026-09-28

Status: implemented; normal-device acceptance pending

The Android approval panel offers “allow and remember”. The service stores
the peer Node ID and current pairing generation in private preferences after
successful approval. Its existing service poll resolves subsequent touchpad
requests for that same active pairing, without requiring an open WebView.
Requests still enter through the authenticated, published-resource path and
receive finite Route grants. Re-pairing invalidates the saved permission.

Settings can clear remembered touchpad permissions. Clearing affects future
requests, not an already active Route; Stop remains the way to end current use.
No microphone/camera permission or global hardware access is implied. No Android
platform permission changes. First trust still uses the existing invitation
file; bidirectional network invitations are not implemented by this change.

Per the current user direction, verification is limited to compilation and
normal application acceptance, without new test suites or ADB verification.
