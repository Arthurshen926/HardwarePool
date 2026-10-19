# Android-initiated audio finite-grant repair

## Failure and cause

Windows-initiated speaker playback was physically confirmed. Android-initiated
speaker use failed at `reciprocal-offer/worker-validation` with
`audio offer finite grant mismatch` after pairing/relaunch had been confirmed.

Android's local-start intent resolved its resource consent with the default
eight-hour expiry on the first peer status, including a Pending acknowledgement.
Windows independently authorized its receiving proposal for one hour. Android
returned its eight-hour authorization in the reciprocal media offer; Windows
correctly rejected unequal authorization bindings.

## Narrow change

For an Android-local audio intent only, wait for an authenticated finite peer
grant before resolving local consent. Use the earlier of that deadline and the
existing Android consent limit. Do not expose a second local consent request
while the peer is still Pending. The original local Start is the user's local
consent; remote approval is not treated as permission for unrelated hardware.

Windows-initiated consent, controller/camera consent, exact worker-offer checks,
leases, TLS, service configuration, pairing and data-plane framing are unchanged.
No permission, driver or dependency change is needed. This is a sequencing repair
within the existing finite-grant contract, not a relaxed grant comparison.

## Checks

- `cargo test --offline -p capyio-android-jni --lib touchpad_consent`: 9 passed,
  including earlier-deadline and expired-peer-grant regressions.
- Initial Android build compiled Rust but NDK linking was denied by the sandbox:
  `ndk/29.0.14206865/.../clang: Permission denied`. Retrying through approved
  escalation succeeded. Both integrated APK and GameStream packaging checks pass.
- Installed the full debug APK on `<LAB_IPV4_01>:45311` with `adb install -r`:
  Success. Revision `audio-peer-grant-20260929`; SHA-256
  `96B5E98C7F8ED61F6F1B98752692118B29AADA94E3A388B64C5A1F8BC9485AE9`.
  No Windows EXE, service, driver or pairing update was performed.
- Android automatically reconnected: one authenticated connection, active catalog.
  Started speaker through the Android UI bridge (normal `startAudioUse` handler),
  with control/data continuing over Tailscale, not an ADB tunnel.
- Route `<LAB_ID_09>` reached `endpointPrepared=true`,
  `shouldRun=true`, `status=starting`, `waitingMedia=true`, `error=null`.
  This passes the formerly failing reciprocal-offer startup stage, but is not
  evidence of audible playback.
- Subsequent Android readback for the same Route reached `status=active`,
  `waitingMedia=false`, `shouldRun=true`, `error=null`: media processing started.
  The user subsequently confirmed audible playback and that stopping works at
  any time. This closes the reported Android-initiated speaker startup defect;
  long-duration and concurrent-capability acceptance remain separate.
