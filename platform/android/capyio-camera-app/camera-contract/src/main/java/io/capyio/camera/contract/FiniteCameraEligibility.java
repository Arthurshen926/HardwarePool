package io.capyio.camera.contract;

import java.util.Objects;

/** Pure, fail-closed metadata gate for the opt-in finite camera profile. No hardware is opened. */
public final class FiniteCameraEligibility {
    public static final String FORMAT_ID = "cavc-avc-finite-720p30-v1";
    public static final int WIDTH = 1280;
    public static final int HEIGHT = 720;
    public static final int FRAMES_PER_SECOND = 30;
    public static final int BITRATE = 4_000_000;
    public static final long MAX_MIN_FRAME_DURATION_NS = 1_000_000_000L / FRAMES_PER_SECOND;

    /** Only facts for the exact selected output size are projected from Camera2 metadata. */
    public record CameraDescriptor(
            String cameraId,
            CameraInventory.LensFacing facing,
            Integer sensorOrientationDegrees,
            boolean dynamicSensorOrientation,
            boolean codecSurface720p,
            boolean previewSurface720p,
            long codecMinFrameDurationNs,
            long previewMinFrameDurationNs,
            boolean fixed30FpsRange,
            boolean explicitMirrorNoneControl,
            boolean rotateAndCropNone) {
        public CameraDescriptor {
            Objects.requireNonNull(cameraId, "cameraId");
            Objects.requireNonNull(facing, "facing");
        }
    }

    /** These are capability-query results, not claims about the actual encoded bitstream. */
    public record EncoderDescriptor(
            String codecName,
            boolean encoder,
            boolean avc,
            boolean surfaceInput,
            boolean baselineLevel31,
            boolean sizeAndRate720p30,
            boolean bitrate4Mbps,
            boolean exactFormatSupported) {
        public EncoderDescriptor {
            Objects.requireNonNull(codecName, "codecName");
        }
    }

    /** Required actual output values; absent/wrong platform keys project to null/false. */
    public record OutputDescriptor(
            String mime, Integer width, Integer height, Integer framesPerSecond, Integer bitrate,
            boolean baselineProfile, boolean level31, boolean surfaceInput, boolean noBFrames,
            boolean bt709, boolean limitedRange, boolean sdrTransfer, boolean unrotatedAndUncropped) {}

    public static void requireOutput(OutputDescriptor output) {
        Objects.requireNonNull(output, "output");
        if (!"video/avc".equals(output.mime()) || !Integer.valueOf(WIDTH).equals(output.width())
                || !Integer.valueOf(HEIGHT).equals(output.height())
                || !Integer.valueOf(FRAMES_PER_SECOND).equals(output.framesPerSecond())
                || !Integer.valueOf(BITRATE).equals(output.bitrate())
                || !output.baselineProfile() || !output.level31() || !output.surfaceInput()
                || !output.noBFrames() || !output.bt709() || !output.limitedRange()
                || !output.sdrTransfer() || !output.unrotatedAndUncropped()) {
            throw new IllegalArgumentException("actual finite AVC format is missing required canonical output facts");
        }
    }

    private FiniteCameraEligibility() {}

    public static void requireCamera(CameraSourceSelection source, CameraDescriptor camera) {
        if (source == null) throw new IllegalArgumentException("finite capture requires an explicit camera");
        Objects.requireNonNull(camera, "camera");
        if (source.targetZoomRatioMilli() != null) {
            throw new IllegalArgumentException("finite capture does not permit a zoom/lens target");
        }
        if (!source.cameraId().equals(camera.cameraId()) || source.facing() != camera.facing()) {
            throw new IllegalArgumentException("finite camera selection changed");
        }
        if (!camera.explicitMirrorNoneControl() || !camera.rotateAndCropNone()) {
            throw new IllegalArgumentException("finite capture requires explicit unmirrored, uncropped Camera2 output control");
        }
        // Every finite frame uses the same pixel transform, including orientation zero.
        // Metadata alone never normalizes encoder output or authorizes a direct-camera path.
        FiniteCameraTransform.plan(WIDTH, HEIGHT, camera.sensorOrientationDegrees(),
                camera.facing(), camera.dynamicSensorOrientation());
        if (!camera.previewSurface720p() || !camera.fixed30FpsRange()) {
            throw new IllegalArgumentException("finite camera requires exact 1280x720 texture input and fixed [30,30] AE");
        }
        if (!supports30Fps(camera.previewMinFrameDurationNs())) {
            throw new IllegalArgumentException("finite camera minimum frame duration cannot prove 30 fps");
        }
    }

    public static void requireEncoder(EncoderDescriptor encoder) {
        Objects.requireNonNull(encoder, "encoder");
        if (encoder.codecName().isEmpty() || encoder.codecName().length() > 256
                || encoder.codecName().chars().anyMatch(Character::isISOControl)
                || !encoder.encoder() || !encoder.avc() || !encoder.surfaceInput()
                || !encoder.baselineLevel31() || !encoder.sizeAndRate720p30()
                || !encoder.bitrate4Mbps() || !encoder.exactFormatSupported()) {
            throw new IllegalArgumentException("finite capture requires an exact surface AVC Baseline level 3.1 encoder");
        }
    }

    public static void requireUnchanged(CameraSourceSelection source, CameraDescriptor originalCamera,
            EncoderDescriptor originalEncoder, CameraDescriptor currentCamera, EncoderDescriptor currentEncoder) {
        requireCamera(source, currentCamera);
        requireEncoder(currentEncoder);
        if (!Objects.requireNonNull(originalCamera, "originalCamera").equals(currentCamera)
                || !Objects.requireNonNull(originalEncoder, "originalEncoder").equals(currentEncoder)) {
            throw new IllegalArgumentException("finite capture plan metadata or pinned identity changed");
        }
    }

    private static boolean supports30Fps(long minimumFrameDurationNs) {
        // Zero means unknown/unsupported, not proof that a stream can run at 30 fps.
        return minimumFrameDurationNs > 0 && minimumFrameDurationNs <= MAX_MIN_FRAME_DURATION_NS;
    }
}
