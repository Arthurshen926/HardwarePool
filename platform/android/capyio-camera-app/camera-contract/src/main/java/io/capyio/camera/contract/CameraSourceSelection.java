package io.capyio.camera.contract;

import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

/**
 * A bounded choice of one directly openable Camera2 device and an optional
 * vendor Zoom target. A Zoom target does not claim a particular physical lens.
 */
public record CameraSourceSelection(
        String cameraId,
        CameraInventory.LensFacing facing,
        Integer targetZoomRatioMilli) {
    public static final int MAX_ZOOM_TARGETS_PER_CAMERA = 3;
    public static final int MAX_SOURCES =
            CameraInventory.MAX_CAMERAS * (MAX_ZOOM_TARGETS_PER_CAMERA + 1);

    public CameraSourceSelection {
        validateId(cameraId, "camera id");
        facing = Objects.requireNonNull(facing, "facing");
        if (targetZoomRatioMilli != null
                && (targetZoomRatioMilli <= 0 || targetZoomRatioMilli > 1_000_000)) {
            throw new IllegalArgumentException("invalid Zoom target");
        }
    }

    public String key() {
        return targetZoomRatioMilli == null
                ? cameraId + "@auto"
                : cameraId + "@" + String.format(
                        java.util.Locale.ROOT,
                        "%.3fx",
                        targetZoomRatioMilli / 1_000.0);
    }

    public static List<CameraSourceSelection> enumerate(CameraInventory inventory) {
        Objects.requireNonNull(inventory, "inventory");
        List<CameraSourceSelection> sources = new ArrayList<>();
        for (CameraInventory.Camera camera : inventory.cameras()) {
            sources.add(new CameraSourceSelection(
                    camera.id(), camera.facing(), null));
            addZoomTargets(sources, camera);
        }
        if (sources.size() > MAX_SOURCES) {
            throw new IllegalArgumentException("camera sources exceed bound");
        }
        return List.copyOf(sources);
    }

    private static void addZoomTargets(
            List<CameraSourceSelection> sources,
            CameraInventory.Camera camera) {
        Integer minimum = camera.minimumZoomRatioMilli();
        Integer maximum = camera.maximumZoomRatioMilli();
        if (minimum == null || maximum == null) {
            return;
        }
        List<Integer> targets = new ArrayList<>(MAX_ZOOM_TARGETS_PER_CAMERA);
        if (minimum < 1_000) {
            targets.add(minimum);
        }
        addIfSupportedAndUnique(targets, 1_000, minimum, maximum);
        addIfSupportedAndUnique(targets, 2_000, minimum, maximum);
        for (Integer target : targets) {
            sources.add(new CameraSourceSelection(camera.id(), camera.facing(), target));
        }
    }

    private static void addIfSupportedAndUnique(
            List<Integer> targets,
            int target,
            int minimum,
            int maximum) {
        if (target >= minimum && target <= maximum && !targets.contains(target)) {
            targets.add(target);
        }
    }

    /** Returns the first source, then each following source, then null for automatic selection. */
    public static CameraSourceSelection next(
            CameraSourceSelection current,
            List<CameraSourceSelection> available) {
        Objects.requireNonNull(available, "available sources");
        if (available.isEmpty() || available.size() > MAX_SOURCES) {
            throw new IllegalArgumentException("available sources must be non-empty and bounded");
        }
        if (current == null) {
            return available.get(0);
        }
        int index = available.indexOf(current);
        if (index < 0 || index + 1 == available.size()) {
            return null;
        }
        return available.get(index + 1);
    }

    private static void validateId(String id, String label) {
        Objects.requireNonNull(id, label);
        if (id.isEmpty() || id.length() > CameraInventory.MAX_ID_CHARS) {
            throw new IllegalArgumentException("invalid " + label + " length");
        }
        for (int index = 0; index < id.length(); index++) {
            if (Character.isISOControl(id.charAt(index))) {
                throw new IllegalArgumentException(label + " contains a control character");
            }
        }
    }
}
