package io.capyio.camera.lab;

import android.content.Context;
import android.graphics.SurfaceTexture;
import android.hardware.camera2.CameraAccessException;
import android.hardware.camera2.CameraCharacteristics;
import android.hardware.camera2.CameraManager;
import android.media.MediaCodec;
import android.os.Build;
import android.util.Range;
import android.util.Size;
import android.util.SizeF;
import io.capyio.camera.contract.CameraInventory;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/** Read-only Camera2 capability inventory. It never opens a camera or allocates an image surface. */
final class CameraInventoryCollector {
    private CameraInventoryCollector() {}

    static CameraInventory collect(Context context) throws CameraAccessException {
        CameraManager manager = context.getSystemService(CameraManager.class);
        String[] cameraIds = manager.getCameraIdList();
        Arrays.sort(cameraIds);
        if (cameraIds.length > CameraInventory.MAX_CAMERAS) {
            throw new IllegalStateException("Camera2 inventory exceeds fixed camera bound");
        }

        List<CameraInventory.Camera> cameras = new ArrayList<>(cameraIds.length);
        for (String cameraId : cameraIds) {
            CameraCharacteristics characteristics = manager.getCameraCharacteristics(cameraId);
            cameras.add(toCamera(manager, cameraId, characteristics));
        }

        List<List<String>> concurrentGroups = new ArrayList<>();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            List<List<String>> groups = new ArrayList<>();
            for (Set<String> ids : manager.getConcurrentCameraIds()) {
                List<String> group = sortedIds(ids, CameraInventory.MAX_CAMERAS_PER_GROUP);
                if (group.size() >= 2) {
                    groups.add(group);
                }
            }
            groups.sort(Comparator.comparing(group -> String.join("\u0000", group)));
            if (groups.size() > CameraInventory.MAX_CONCURRENT_GROUPS) {
                throw new IllegalStateException("Camera2 inventory exceeds concurrent group bound");
            }
            concurrentGroups.addAll(groups);
        }
        return new CameraInventory(cameras, concurrentGroups);
    }

    private static CameraInventory.Camera toCamera(
            CameraManager manager,
            String cameraId,
            CameraCharacteristics characteristics) throws CameraAccessException {
        Set<String> physicalIds = characteristics.getPhysicalCameraIds();
        List<String> sortedPhysicalIds = sortedIds(
                physicalIds, CameraInventory.MAX_PHYSICAL_CAMERAS);
        float[] focalLengths = characteristics.get(CameraCharacteristics.LENS_INFO_AVAILABLE_FOCAL_LENGTHS);
        List<Integer> focalLengthMilliMm = new ArrayList<>();
        if (focalLengths != null) {
            Arrays.sort(focalLengths);
            int limit = Math.min(focalLengths.length, CameraInventory.MAX_FOCAL_LENGTHS);
            for (int index = 0; index < limit; index++) {
                float focalLength = focalLengths[index];
                if (Float.isFinite(focalLength) && focalLength > 0) {
                    focalLengthMilliMm.add(Math.round(focalLength * 1_000f));
                }
            }
        }

        Integer minimumZoomRatioMilli = null;
        Integer maximumZoomRatioMilli = null;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Range<Float> zoomRange = characteristics.get(CameraCharacteristics.CONTROL_ZOOM_RATIO_RANGE);
            if (zoomRange != null
                    && Float.isFinite(zoomRange.getLower())
                    && Float.isFinite(zoomRange.getUpper())) {
                minimumZoomRatioMilli = Math.round(zoomRange.getLower() * 1_000f);
                maximumZoomRatioMilli = Math.round(zoomRange.getUpper() * 1_000f);
            }
        }

        Integer orientation = characteristics.get(CameraCharacteristics.SENSOR_ORIENTATION);
        return new CameraInventory.Camera(
                cameraId,
                facing(characteristics.get(CameraCharacteristics.LENS_FACING)),
                hardwareLevel(characteristics.get(CameraCharacteristics.INFO_SUPPORTED_HARDWARE_LEVEL)),
                normalizeOrientation(orientation),
                sortedPhysicalIds,
                physicalLenses(manager, sortedPhysicalIds),
                focalLengthMilliMm,
                minimumZoomRatioMilli,
                maximumZoomRatioMilli,
                commonSizes(characteristics));
    }

    private static List<CameraInventory.PhysicalLens> physicalLenses(
            CameraManager manager,
            List<String> physicalIds) throws CameraAccessException {
        List<CameraInventory.PhysicalLens> lenses = new ArrayList<>(physicalIds.size());
        for (String physicalId : physicalIds) {
            CameraCharacteristics physical = manager.getCameraCharacteristics(physicalId);
            float[] focalLengths = physical.get(CameraCharacteristics.LENS_INFO_AVAILABLE_FOCAL_LENGTHS);
            List<Integer> focalLengthMilliMm = new ArrayList<>();
            if (focalLengths != null) {
                Arrays.sort(focalLengths);
                int limit = Math.min(focalLengths.length, CameraInventory.MAX_FOCAL_LENGTHS);
                for (int index = 0; index < limit; index++) {
                    float focalLength = focalLengths[index];
                    if (Float.isFinite(focalLength) && focalLength > 0) {
                        focalLengthMilliMm.add(Math.round(focalLength * 1_000f));
                    }
                }
            }
            SizeF sensorSize = physical.get(CameraCharacteristics.SENSOR_INFO_PHYSICAL_SIZE);
            Integer sensorWidthMicroMm = null;
            Integer sensorHeightMicroMm = null;
            if (sensorSize != null
                    && Float.isFinite(sensorSize.getWidth())
                    && Float.isFinite(sensorSize.getHeight())
                    && sensorSize.getWidth() > 0
                    && sensorSize.getHeight() > 0) {
                sensorWidthMicroMm = Math.round(sensorSize.getWidth() * 1_000f);
                sensorHeightMicroMm = Math.round(sensorSize.getHeight() * 1_000f);
            }
            lenses.add(new CameraInventory.PhysicalLens(
                    physicalId,
                    focalLengthMilliMm,
                    sensorWidthMicroMm,
                    sensorHeightMicroMm));
        }
        return List.copyOf(lenses);
    }

    private static List<CameraInventory.Size> commonSizes(CameraCharacteristics characteristics) {
        android.hardware.camera2.params.StreamConfigurationMap map =
                characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP);
        if (map == null) {
            return List.of();
        }
        Size[] encoderSizes = map.getOutputSizes(MediaCodec.class);
        Size[] previewSizes = map.getOutputSizes(SurfaceTexture.class);
        if (encoderSizes == null || previewSizes == null) {
            return List.of();
        }
        Set<String> previewKeys = new HashSet<>();
        for (Size size : previewSizes) {
            previewKeys.add(size.getWidth() + "x" + size.getHeight());
        }
        List<Size> common = new ArrayList<>();
        for (Size size : encoderSizes) {
            if (previewKeys.contains(size.getWidth() + "x" + size.getHeight())) {
                common.add(size);
            }
        }
        common.sort(Comparator.comparingLong(
                        (Size size) -> (long) size.getWidth() * size.getHeight())
                .thenComparingInt(Size::getWidth)
                .thenComparingInt(Size::getHeight));
        int limit = Math.min(common.size(), CameraInventory.MAX_COMMON_SIZES);
        List<CameraInventory.Size> result = new ArrayList<>(limit);
        for (int index = 0; index < limit; index++) {
            Size size = common.get(index);
            result.add(new CameraInventory.Size(size.getWidth(), size.getHeight()));
        }
        return List.copyOf(result);
    }

    private static List<String> sortedIds(Set<String> ids, int maximum) {
        List<String> sorted = new ArrayList<>(ids);
        sorted.sort(String::compareTo);
        if (sorted.size() > maximum) {
            return List.copyOf(sorted.subList(0, maximum));
        }
        return List.copyOf(sorted);
    }

    private static CameraInventory.LensFacing facing(Integer value) {
        if (value == null) {
            return CameraInventory.LensFacing.UNKNOWN;
        }
        return switch (value) {
            case CameraCharacteristics.LENS_FACING_FRONT -> CameraInventory.LensFacing.FRONT;
            case CameraCharacteristics.LENS_FACING_BACK -> CameraInventory.LensFacing.BACK;
            case CameraCharacteristics.LENS_FACING_EXTERNAL -> CameraInventory.LensFacing.EXTERNAL;
            default -> CameraInventory.LensFacing.UNKNOWN;
        };
    }

    private static String hardwareLevel(Integer value) {
        if (value == null) {
            return "unknown";
        }
        return switch (value) {
            case CameraCharacteristics.INFO_SUPPORTED_HARDWARE_LEVEL_LEGACY -> "legacy";
            case CameraCharacteristics.INFO_SUPPORTED_HARDWARE_LEVEL_LIMITED -> "limited";
            case CameraCharacteristics.INFO_SUPPORTED_HARDWARE_LEVEL_FULL -> "full";
            case CameraCharacteristics.INFO_SUPPORTED_HARDWARE_LEVEL_3 -> "level3";
            case CameraCharacteristics.INFO_SUPPORTED_HARDWARE_LEVEL_EXTERNAL -> "external";
            default -> "unknown";
        };
    }

    private static int normalizeOrientation(Integer orientation) {
        if (orientation == null) {
            return 0;
        }
        return switch (orientation) {
            case 0, 90, 180, 270 -> orientation;
            default -> 0;
        };
    }
}
