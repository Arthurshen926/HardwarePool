# CAPY-CAMERA-001C0 Android Camera2 build-only report

- Date: 2026-08-30
- Branch: `codex/capyio-camera`
- Base commit: `fc3da3636ca6c969667e71a9b596dcc944380146`
- Scope: first visible Android Camera2 capture boundary; no APK installation

## Outcome

The repository now contains a buildable Android Camera2 Lab APK. It can request
the camera permission after a user presses Start, select a back camera when
available, show a `TextureView` preview and observe latest `YUV_420_888` image
metadata through a two-image `ImageReader`. Activity pause, preview destruction,
explicit stop and capture failure all close the owned Camera2 resources.

This is build and static-contract evidence only. The APK was not installed and
no physical camera was opened, so it does not yet prove a valid phone image or
Android-to-Windows sharing.

## Boundaries

- Manifest permissions: exactly `android.permission.CAMERA`.
- No Internet, storage, microphone, location or foreground-service permission.
- No Android service component, network listener, persistence or frame log.
- The user must explicitly start; capture stops when the Activity is hidden.
- `FLAG_SECURE` protects the visible preview from ordinary screenshots.
- Each acquired image closes synchronously; no pixel payload enters a control
  envelope or the Rust Core.
- Camera2 YUV plane layout is not claimed to be packed NV12. MediaCodec and the
  encoded transport remain later gates.

## Automated evidence

The following command passed:

```text
gradle contractTest :app:assembleDebug :app:lintDebug
```

The executable no-dependency contract tests cover permission grant/deny,
visible start, pause close, failure cleanup/retry and invalid frame metadata.
Android lint passes with warnings treated as errors. The Android API 36 pin is
the only locally disabled currency check because toolchain updates require a
separate audited task.

`aapt2 dump permissions` reports:

```text
package: io.capyio.camera.lab
uses-permission: name='android.permission.CAMERA'
```

The generated debug APK was 2,589,866 bytes with SHA-256:

```text
ee38835906613ab9a46aaec5ad71364c8cd329e69359fcc5249761c9b65b5d28
```

The build downloaded official missing AGP artifacts and Android SDK Build Tools
36.0.0. It did not invoke ADB or install the APK.

## Remaining gates

1. Approve an exact ADB serial and APK install/rollback command.
2. On the visible device, grant CAMERA and verify preview, changing positive
   timestamps/sequences, lens facing, resolution and pause/resume cleanup.
3. Define and implement a bounded MediaCodec H.264 access-unit output contract.
4. Add authenticated transport and a Windows decoder feeding `camera-host`.
5. Switch the registered MF class factory from its fixture to the shared frame
   provider and repeat the approved Windows camera roundtrip.
