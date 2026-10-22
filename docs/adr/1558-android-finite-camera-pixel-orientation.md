# ADR 1558: Finite Android camera pixel orientation

Status: implementation and hardware-free validation; no physical-phone evidence.

## Decision and compatibility

This supersedes ADR 1557's orientation-zero-only restriction, not its explicit
opt-in, exact output validation, one-shot generation, or finite authority rules.
Legacy remains the default and its direct Camera2-to-codec path is unchanged.
No wire format, permission, foreground service, JNI media contract, resource
approval, lease duration, renewal or automatic fallback is introduced.

The experimental finite path uses a real intermediate GPU pixel stage:
Camera2 → SurfaceTexture / external-OES texture → EGL → MediaCodec input Surface.
Input is exactly 1280×720. Output remains exactly 1280×720, rotation zero,
30 fps AVC Baseline level 3.1, BT.709 limited-range SDR. The actual MediaFormat,
SPS/PPS/AU, Java reservation, Rust sender and Linux receiver gates remain required.
A successful shader draw is not encoded-output validation or consumer delivery.
CaptureResult rotate-and-crop NONE is additionally checked when its callback
arrives; that callback is not atomically associated with presentation. The
per-latched-buffer matrix is checked before drawing, but neither check
authenticates HAL pixel content or guarantees a bad first HAL frame was withheld.

The immutable capture Plan pins the selected camera, encoder, input dimensions,
static cardinal sensor orientation, facing, no-mirror policy and aspect-fit
policy. Missing/non-cardinal/dynamic orientation and changed metadata still fail
closed. The same Plan is revalidated before camera open. Finite capture requires
Android 13+ so the camera output can explicitly select `MIRROR_MODE_NONE`;
there is no older-API mirror guess or silent downgrade. Explicit
`SCALER_ROTATE_AND_CROP_NONE` avoids a second camera rotate-and-crop policy.
Eligibility describes an attemptable configuration, not a verified device.

## Rotation is applied exactly once

Android's camera service includes sensor rotation in the buffer transform.
`SurfaceTexture.getTransformMatrix()` includes that transform, crop adjustments
and the GL/buffer vertical-coordinate conversion. Default AUTO mode additionally
mirrors front-facing camera outputs. Therefore adding a manual sensor rotation
to that matrix would rotate twice. Finite mode explicitly disables mirroring,
checks the actual matrix against the pinned orientation's allowed family and
applies it once. Unexpected reflection, rotation, shear, perspective or non-finite
values are rejected rather than normalized heuristically. Texture filter insets of at most two raw texels on each edge are allowed;
arbitrary content crop or stretch is not.

The output is upright relative to the device's natural orientation, not its
current display rotation. Front cameras are not selfie-mirrored. There is no
live display-orientation tracking, sensor fusion or fold-state substitution.
Changing the selected source/profile still stops and invalidates the old use
before a fresh catalog can be published; it cannot widen existing consent.

For 0/180 degrees the fitted image fills the output. For 90/270 degrees the full
rotated 720×1280 image is fitted without stretch into the 1280×720 landscape
canvas: ideal content width 405 pixels, centered at x=437.5 through x=842.5.
Black opaque sidebars remain. This deliberately sacrifices horizontal resolution
rather than cropping portrait content, pretending it is landscape, or changing
the negotiated profile. Rasterization at those exact half-pixel edges is tested
separately from interior pixel correctness.

## Lifetime and resource boundary

EGL runs on a dedicated owner thread, not the Camera2 handler that runs the
existing progress watchdog. The application keeps a bounded, coalesced latest
frame notification, not a per-frame render queue or whole-session pixel copies.
Each draw reads the current producer transform and a positive, strictly advancing
producer timestamp, with explicit SENSOR timestamp base. A late coalesced callback
can latch the same current buffer again: that exact repeat is dropped without
presentation or progress refresh. Regressing timestamps and distinct nanoseconds
colliding at encoder microsecond precision fail closed. Presentation timestamps
are not reset to make a stale frame new. PTS never renews authority. Original route/generation liveness is checked
around rendering and presentation; the native sender independently enforces
original authority and rate/work ceilings.

Initialization, render failure and Stop are terminal for that transform owner.
Camera production stops before owned GL input resources are released. GL teardown
runs on its owning thread, and encoder Surface cleanup follows GL release rather
than racing an in-flight swap. Failures immediately stop secure writes before
asynchronous resource cleanup. Stop does not wait under the service monitor for
a GPU driver call. A blocked driver call cannot be forcibly interrupted by this
Java design: bounded application queues and deadlines are not a hard bound on
driver call latency, driver memory, or successful driver teardown. Resources must
not be released under a still-running GL call merely to claim bounded cleanup.

The existing 32 MiB stream byte ceiling, finite frame/rate budgets and Linux native
CPU limits remain. This is not unlimited streaming and does not ensure every
eligible phone can sustain 30 fps or the maximum approved deadline.

## Evidence and limits

Portable Java tests exercise the production transform/matrix and lifecycle
contract with synthetic inputs, invalid producer matrices/timestamps and fake
resource failures. Hosted CI compiles the actual Android code against the existing
SDK and ARM64 JNI. A separate mandatory Mesa software EGL/GLES test exports the
actual production shader/matrix data, imports a synthetic asymmetric RGBA image
through EGLImage into an external-OES texture, renders and reads back pixels.
Its coordinate oracle is independently implemented; no sampler2D substitute or
missing-extension skip counts as proof. It checks every cardinal orientation,
no-mirror behavior, crop insets, opaque bars and defined edge behavior.

That host test proves the shared GPU algorithm. It does not execute Android
Camera2, a phone SurfaceTexture producer, MediaCodec or a physical sensor, and
does not establish device cadence, camera color accuracy, phone performance,
thermal behavior or broad hardware compatibility. Existing production Rust
sender-to-Linux/PipeWire synthetic graph proofs remain separate and mandatory.
No phone capture, deployment, user keyring access or persistent credential creation
is performed by these tests.

The production path uses existing Android framework EGL/GLES/Camera2 APIs,
not a new bundled dependency. The hosted-only oracle links official Ubuntu Mesa
EGL/GLES packages in its disposable runner; no Mesa source or binaries are vendored
or shipped in the Android product. Software rendering is forced and checked.

## Primary references

- [OutputConfiguration](https://developer.android.com/reference/android/hardware/camera2/params/OutputConfiguration): API 33 mirror policy and SurfaceTexture transform semantics.
- [SurfaceTexture](https://developer.android.com/reference/android/graphics/SurfaceTexture): current-context update, per-image transform and producer timestamps.
- [AOSP CameraUtils](https://android.googlesource.com/platform/frameworks/av/+/master/camera/CameraUtils.cpp): no-mirror camera rotation transform.
- [AOSP GLConsumer transform](https://android.googlesource.com/platform/frameworks/native/+/master/libs/gui/GLConsumerUtils.cpp): buffer flags, crop and final vertical conversion.
- [CameraX SurfaceOutputImpl](https://android.googlesource.com/platform/frameworks/support/+/6353c5d0cbdb0a766f9640dde3ffcb0f9e4cfcf1/camera/camera-core/src/main/java/androidx/camera/core/processing/SurfaceOutputImpl.java): camera producer rotation/mirroring is already in the SurfaceTexture matrix.
