package io.capyio.camera.lab;

import android.content.Context;
import android.graphics.SurfaceTexture;
import android.hardware.camera2.CameraAccessException;
import android.hardware.camera2.CameraCharacteristics;
import android.hardware.camera2.CameraManager;
import android.hardware.camera2.params.StreamConfigurationMap;
import android.media.MediaCodec;
import android.media.MediaCodecInfo;
import android.media.MediaCodecList;
import android.media.MediaFormat;
import android.os.Build;
import android.util.Range;
import android.util.Size;
import io.capyio.camera.contract.AvcEncoderConfig;
import io.capyio.camera.contract.CameraInventory;
import io.capyio.camera.contract.CameraSourceSelection;
import io.capyio.camera.contract.FiniteCameraEligibility;
import java.util.Arrays;
import java.util.Objects;

/** Metadata-only capability gate. It never opens a camera, codec, Surface or transport. */
public final class FiniteCameraPreflight {
    private static final Size SIZE = new Size(FiniteCameraEligibility.WIDTH, FiniteCameraEligibility.HEIGHT);

    /** In-process immutable proof candidate, revalidated before capture; it is not device evidence. */
    public static final class Plan {
        private final CameraSourceSelection source;
        private final FiniteCameraEligibility.CameraDescriptor camera;
        private final FiniteCameraEligibility.EncoderDescriptor encoder;
        private final AvcEncoderConfig config = AvcEncoderConfig.baseline720p30();

        private Plan(CameraSourceSelection source, FiniteCameraEligibility.CameraDescriptor camera,
                FiniteCameraEligibility.EncoderDescriptor encoder) {
            this.source = source;
            this.camera = camera;
            this.encoder = encoder;
        }

        public String cameraId() { return camera.cameraId(); }
        public String codecName() { return encoder.codecName(); }
        public CameraSourceSelection source() { return source; }
        public AvcEncoderConfig encoderConfig() { return config; }
        public String formatId() { return FiniteCameraEligibility.FORMAT_ID; }
    }

    private FiniteCameraPreflight() {}

    public static Plan preflight(Context context, CameraSourceSelection source) throws CameraAccessException {
        Objects.requireNonNull(context, "context");
        if (source == null || source.targetZoomRatioMilli() != null) {
            throw new IllegalArgumentException("finite capture requires an explicit camera without zoom target");
        }
        FiniteCameraEligibility.CameraDescriptor camera = describeCamera(context, source.cameraId());
        FiniteCameraEligibility.requireCamera(source, camera);
        for (MediaCodecInfo info : new MediaCodecList(MediaCodecList.REGULAR_CODECS).getCodecInfos()) {
            if (!info.isEncoder() || !supportsAvc(info)) continue;
            try {
                FiniteCameraEligibility.EncoderDescriptor encoder = describeEncoder(info);
                FiniteCameraEligibility.requireEncoder(encoder);
                return new Plan(source, camera, encoder);
            } catch (IllegalArgumentException | IllegalStateException unsupported) {
                // Metadata candidates only; no codec is created or attempted during this probe.
            }
        }
        throw new IllegalStateException("no exact finite AVC encoder is advertised");
    }

    /** Recheck only the original named camera and encoder. Never select a replacement. */
    public static void revalidate(Context context, Plan plan) throws CameraAccessException {
        Objects.requireNonNull(plan, "plan");
        FiniteCameraEligibility.CameraDescriptor camera = describeCamera(context, plan.cameraId());
        FiniteCameraEligibility.requireCamera(plan.source(), camera);
        if (!plan.camera.equals(camera)) throw new IllegalStateException("finite camera metadata changed");
        for (MediaCodecInfo info : new MediaCodecList(MediaCodecList.REGULAR_CODECS).getCodecInfos()) {
            if (!plan.codecName().equals(info.getName())) continue;
            FiniteCameraEligibility.EncoderDescriptor encoder = describeEncoder(info);
            FiniteCameraEligibility.requireUnchanged(plan.source(), plan.camera, plan.encoder, camera, encoder);
            return;
        }
        throw new IllegalStateException("pinned finite AVC encoder is unavailable");
    }

    private static FiniteCameraEligibility.CameraDescriptor describeCamera(Context context, String cameraId)
            throws CameraAccessException {
        CameraManager manager = Objects.requireNonNull(context.getSystemService(CameraManager.class), "CameraManager");
        if (!Arrays.asList(manager.getCameraIdList()).contains(cameraId)) {
            throw new IllegalArgumentException("finite camera is not directly openable");
        }
        CameraCharacteristics characteristics = manager.getCameraCharacteristics(cameraId);
        StreamConfigurationMap map = characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP);
        if (map == null) throw new IllegalArgumentException("finite camera stream metadata is missing");
        boolean codecSize = contains(map.getOutputSizes(MediaCodec.class), SIZE);
        boolean previewSize = contains(map.getOutputSizes(SurfaceTexture.class), SIZE);
        boolean fixed30 = false;
        Range<Integer>[] ranges = characteristics.get(CameraCharacteristics.CONTROL_AE_AVAILABLE_TARGET_FPS_RANGES);
        if (ranges != null) {
            for (Range<Integer> range : ranges) {
                if (range != null && range.getLower() == 30 && range.getUpper() == 30) fixed30 = true;
            }
        }
        boolean dynamicOrientation = Build.VERSION.SDK_INT >= 32
                && characteristics.get(CameraCharacteristics.INFO_DEVICE_STATE_SENSOR_ORIENTATION_MAP) != null;
        return new FiniteCameraEligibility.CameraDescriptor(cameraId,
                facing(characteristics.get(CameraCharacteristics.LENS_FACING)),
                characteristics.get(CameraCharacteristics.SENSOR_ORIENTATION), dynamicOrientation,
                codecSize, previewSize,
                codecSize ? map.getOutputMinFrameDuration(MediaCodec.class, SIZE) : 0,
                previewSize ? map.getOutputMinFrameDuration(SurfaceTexture.class, SIZE) : 0,
                fixed30);
    }

    private static FiniteCameraEligibility.EncoderDescriptor describeEncoder(MediaCodecInfo info) {
        boolean avc = supportsAvc(info);
        if (!info.isEncoder() || !avc) {
            return new FiniteCameraEligibility.EncoderDescriptor(info.getName(), info.isEncoder(), avc,
                    false, false, false, false, false);
        }
        MediaCodecInfo.CodecCapabilities caps = info.getCapabilitiesForType(MediaFormat.MIMETYPE_VIDEO_AVC);
        boolean surface = false;
        for (int color : caps.colorFormats) {
            if (color == MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface) surface = true;
        }
        boolean baseline = false;
        for (MediaCodecInfo.CodecProfileLevel level : caps.profileLevels) {
            if (level.profile == MediaCodecInfo.CodecProfileLevel.AVCProfileBaseline
                    && level.level >= MediaCodecInfo.CodecProfileLevel.AVCLevel31) baseline = true;
        }
        MediaCodecInfo.VideoCapabilities video = caps.getVideoCapabilities();
        boolean sizeAndRate = video != null && video.areSizeAndRateSupported(SIZE.getWidth(), SIZE.getHeight(), 30);
        boolean bitrate = video != null && video.getBitrateRange().contains(FiniteCameraEligibility.BITRATE);
        return new FiniteCameraEligibility.EncoderDescriptor(info.getName(), true, true, surface, baseline,
                sizeAndRate, bitrate, caps.isFormatSupported(MediaCodecSurfaceEncoder.createFiniteFormat()));
    }

    private static boolean supportsAvc(MediaCodecInfo info) {
        for (String type : info.getSupportedTypes()) {
            if (MediaFormat.MIMETYPE_VIDEO_AVC.equalsIgnoreCase(type)) return true;
        }
        return false;
    }

    private static boolean contains(Size[] sizes, Size expected) {
        return sizes != null && Arrays.asList(sizes).contains(expected);
    }

    private static CameraInventory.LensFacing facing(Integer facing) {
        if (facing == null) return CameraInventory.LensFacing.UNKNOWN;
        return switch (facing) {
            case CameraCharacteristics.LENS_FACING_FRONT -> CameraInventory.LensFacing.FRONT;
            case CameraCharacteristics.LENS_FACING_BACK -> CameraInventory.LensFacing.BACK;
            case CameraCharacteristics.LENS_FACING_EXTERNAL -> CameraInventory.LensFacing.EXTERNAL;
            default -> CameraInventory.LensFacing.UNKNOWN;
        };
    }
}
