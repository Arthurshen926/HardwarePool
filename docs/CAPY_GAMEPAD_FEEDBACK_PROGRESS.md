# Gamepad feedback integration progress

## Current status — 0.5.21 (supersedes historical missing-worker notes below)

The dedicated, separately authorized haptics Route is now implemented, including
Windows game-feedback Source, Android service-owned TLS Sink, per-resource
consent, local gain, exact execution acknowledgements, lease renewal and bounded
stop on loss. DS4 and Xbox use independent stop-before-switch projections.

Verified this update:

- TLS transport and command suite: 30 passed.
- Haptics control-offer roundtrip/rejection: passed.
- Shared UI resource support: 3 passed; Vue production build passed.
- Android composition: 329 assertions passed.
- Desktop embedded-UI build passed (`integrated-0521-haptics-route`).
- APK build passed after SDK access escalation; installed on
  `<LAB_IPV4_01>:38121`, confirmed versionCode 33 / 0.5.21-dev.
- Same `target/debug/capyio-desktop.exe` launched; native window inspection
  succeeded but activation failed twice (`failed to activate captured window`).
  Real-window action validation is waiting for the user to activate the desktop.

Not yet accepted: end-to-end game rumble reaching the tablet in each mode,
0/50/100% tactile comparison, stop/disconnect and mode switching through both
real UIs. Windows feedback counters and Android execution counts have distinct
labels; neither alone proves perceived vibration. Existing pairing and drivers
were not replaced. Application traffic must use Tailscale, not ADB forwarding.

## 2026-09-29 implemented slice

The user confirmed that Steam receives DS4 motion after the helper clock fix.
This is application-level motion evidence, not full gesture/axis/rumble acceptance.

Windows now polls DS4 output feedback without waiting for the game. The adapter
assembles one fixed seven-byte record across nonblocking reads, retains partial
records, and rejects a partial record that exceeds the existing I/O deadline.
Socket blocking mode is restored before forward input writes. The existing
explicit blocking feedback API uses the same assembly state, so switching APIs
does not lose framing. A feedback failure uses the existing owned projection
cleanup; no unrelated Route is stopped.

The desktop worker reads at most four feedback records per tick and exposes
received-frame count plus the two raw motor values. The UI explicitly says this
is Windows reception only, not delivery to the Android vibrator. Counters reset
on start, and motor values reset on stop.

## Verification

- DS4 session fixture: three tests pass, including silent-game, partial-feedback
  and complete-feedback cases followed by forward input and owned cleanup.
- Desktop gamepad tests: 22 passed, one physical gate ignored before the final
  assembly change; final compile/check must also be retained.
- Vue typecheck/build and desktop cargo check passed before final assembly change.
- No new APK, permission, driver, pairing or remote haptics authority was added.

## Remaining work, not claimed complete

1. Bind feedback to a separately authorized haptics Source-to-Sink Route, as
   required by INPUT_PROFILE.md. A controller receiver token alone must not
   implicitly authorize an actuator.
2. Android opt-in/strength setting, bounded-duration playback and explicit stop
   on Route stop, expiry, disconnect or feedback loss. Combine two game motors
   onto the device's available actuator honestly; do not claim two physical motors.
3. Physical dual-mode switch validation through the desktop UI. Standalone Xbox
   and stop-before-switch are now implemented as described below.
4. Actual Steam/game output -> Windows feedback -> Android vibration validation.
   Do not replace this with a local vibration test button or mock reports.

## Follow-up implementation, not deployed as a complete release

- Desktop now selects DS4 or standalone Xbox 360. Switching stops the owned
  target before starting the other; Xbox creates no VIIPER/USB-IP device. DSU
  remains mutually exclusive. A late read-only probe cannot overwrite Active.
- The ViGEm helper emits small/large motor bytes after its readiness line. Its
  driver callback uses an atomic latest-value mailbox, not pipe writes. The Rust
  owner consumes output on a separate reader and tears down its child on errors.
- Android's existing local haptics Sink now uses a persisted 0–100% gain (70%
  default). Zero cancels; changing gain cancels the previous effect. Both the
  settings page and controller page expose it. This does NOT enable networking.
- The missing independent network feedback Route is recorded in proposed ADR
  1498. A shortcut reusing the input token was not implemented.

### Actual checks (2026-09-29)

- Pinned NuGet 1.21.256 package digest matched the existing third-party record.
  Staged managed DLL SHA-256:
  `4458301000B732D115521E99F9936F4EDB70D6CEB3036EF158715E0E6B8902E0`.
- `FeedbackProbe.cs` created its own Xbox target on the installed ViGEmBus,
  verified A/trigger/stick via XInputGetState, issued XInputSetState and received
  the corresponding small/large bytes, then confirmed device removal. Output:
  `XINPUT_INPUT_AND_GAME_FEEDBACK_PASSED`, `XINPUT_OWNED_DEVICE_REMOVED`.
  This is real local driver/API evidence, not Android network or tactile evidence.
- Windows input unit tests: 2 passed. Final desktop gamepad tests: 24 passed,
  one device-dependent test ignored, including both preflight regressions.
- Android module composition: 324 assertions passed; debug APK built.
- Read-only Y700 vibrator inventory reports vibrator 0 with AMPLITUDE_CONTROL.
  This establishes hardware capability only, not perceived feedback delivery.
- Shared UI typecheck/build passed after the final settings-page placement.

No APK was installed and the main desktop EXE was not replaced by this follow-up.
The user requested one combined validation after the full feature is ready.

## Independent actuator consent slice

Android now recognizes incoming haptics Sink requests separately from outgoing
touchpad requests. Consent sets a lease policy on the local Sink. The request UI
identifies the exact resource; remembered permission uses peer + Capability +
pairing generation, with touchpad-only migration of old entries. It does not
implicitly authorize vibration when a touchpad was previously remembered.

Five native consent tests (including actuator denial and exclusive Sink usage)
passed; all seven Android JNI host tests passed. Both consent UI parser tests
and the shared UI build passed. ARM64 JNI and the final APK built successfully;
the Android composition suite passed 329 assertions, including legacy permission
isolation and re-pair generation invalidation. No APK installed.

The network haptics worker remains missing; remote Start reports an explicit
Offline problem instead of control-only Active. No game-to-tablet vibration or
full mode-switch physical acceptance is claimed by these checks.
