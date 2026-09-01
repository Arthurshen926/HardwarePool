# CAPY-CAMERA-001C28: Background-ownership foundation and lab lifecycle repair

## Outcome

This slice separates Android capture from the Activity preview without yet
changing the manifest. `Camera2Session` accepts an optional `TextureView` and
always retains the bounded MediaCodec output, so a future foreground service
can own an encoder-only stream independently of Activity surface destruction.

`CaptureOwnershipStateMachine` records the intended service-owned lifecycle:
Activity pause, resume and configuration change have no stop effect; only an
explicit user stop or service failure requests service shutdown. The existing
Activity still uses the earlier visible-only state machine, so this report does
not claim working background or lock-screen capture.

The Windows GUI/live verification hold is now a fixed 180 seconds. The elevated
deployment helper has a new explicit `RemoveWithFrameServerRestart` action for
the observed case where Windows Frame Server retains the COM DLL. It restarts
FrameServer in `finally` when and only when that explicit action was chosen.

## Validation

- `:camera-contract:contractTest`: PASS.
- `cargo test -p capyio-windows-camera-mf --bin capyio-camera-virtual-lab`:
  PASS, 6 tests.
- `python scripts/validate_repository.py`: PASS, 84 unique Requirement IDs.
- `:app:assembleDebug`: BLOCKED by repeated local
  `AccessDeniedException` opening the generated
  `app/build/intermediates/compile_r_class_jar/debug/generateDebugRFile/R.jar`.
  Stopping Gradle daemons and deleting only that generated jar did not clear the
  host file-access condition. No Java compiler diagnostic for the changed code
  was emitted.

## Remaining gated work

1. Add the Android camera foreground-service permission and service declaration.
2. Implement the service, notification channel and explicit start/stop commands.
3. Move session/transport ownership from `MainActivity` to that service and
   project state back to the UI.
4. Rebuild, exact-hash, install with explicit approval, then verify live Windows
   pixels across background/foreground, portrait/landscape and producer recovery.
5. Run the explicit Windows cleanup path and confirm FrameServer returns to its
   pre-run state.
