# CAPY-CAMERA-001B1H Report

- Date: 2026-08-29
- Branch: `codex/capyio-camera`
- Baseline: `fc3da3636ca6c969667e71a9b596dcc944380146`
- Scope: extract the Windows camera-share ABI and add producer lifecycle
- System mutation: none

## Direct capability status

The repository still cannot capture the Android phone camera and show that live
image through the Windows virtual camera.

The Windows decoded-frame path now reaches a non-registered MF sample across
processes, but the Android side has no Gradle project, manifest, Camera2,
MediaCodec or camera transport. The registered Windows class factory also
remains fixture-backed.

Repository and branch inventory confirmed that `codex/capyio-android-node`
still points at the common `fc3da36` contract baseline. No later Android camera
implementation branch exists locally or on the listed remotes.

## Outcome

B1H establishes the correct Windows dependency direction:

- `capyio-windows-camera-share` owns the fixed v1 mapping ABI, protected
  producer creation, read-only consumer and cross-process raw-frame tests;
- `capyio-windows-camera-mf` depends only on that shared contract and retains
  the cross-process platform-sample test;
- `capyio-windows-camera-host` owns explicit producer start, validated frame
  publication, accounting, stop and Drop cleanup;
- portable `capyio-runtime` remains free of Win32 dependencies;
- the audio-specific `CapyIOBroker` remains unchanged.

The consumer can now open the current production mapping and adopt its stream
ID and positive epoch from the fully validated ACL-protected header. This
removes the need for a command-line or environment identity channel when a
later registered activation opens the producer mapping.

The optional cross-crate test surface is feature-gated and accepts only bounded
`Local\\CapyIO.CameraIngress.v1.test.*` names. Production creation/opening
remains fixed to `Global\\CapyIO.CameraIngress.v1`.

## Automated evidence

Targeted tests and strict linting passed:

```text
cargo test -p capyio-windows-camera-share --all-targets
cargo test -p capyio-windows-camera-host --all-targets
cargo test -p capyio-windows-camera-mf --all-targets
cargo clippy -p capyio-windows-camera-share -p capyio-windows-camera-host -p capyio-windows-camera-mf --all-targets --all-features -- -D warnings
```

The host tests verify:

- invalid zero epoch and publish while stopped fail closed;
- one host starts, publishes and reports its exact publication count;
- the read side adopts and reports the producer-bound stream ID/epoch;
- explicit stop releases the final mapping and repeated stop is idempotent;
- a second owner is rejected without entering Active state;
- the second owner can retry successfully after the first releases.

The final repository gate also passed:

```text
cargo xtask ci
```

## Safety and rollback

No production global mapping was started by tests. No DLL was deployed, no
virtual camera was registered, no Windows service/driver was changed and no APK
or Android permission was added. Test mappings were local and disappeared after
their final handles closed, so no rollback command was required.

## Remaining gates to live Android video

1. Create an Android application/module with a user-visible Camera2 lifecycle.
   Adding `android.permission.CAMERA` and any foreground-service declaration
   requires explicit human approval before editing the manifest.
2. Select and review an encoded transport and Windows decoder, or define a
   different bounded private Adapter data plane. Raw 720p30 frames must not use
   JSON-RPC/stdout.
3. Connect decoded frames to `CameraProducerHost` only after Route/peer
   authorization.
4. Add a real background camera-host executable/service identity and verify the
   production global mapping ACL.
5. Switch the registered COM activation from fixture to shared consumer in a
   separate gate.
6. Run the approved Android-device capture and Windows system-camera roundtrip,
   then verify exact rollback and privacy cleanup.
