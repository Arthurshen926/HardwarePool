package io.capyio.camera.contract;

import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

/** Bounded, platform-neutral description of Camera2 devices and concurrency groups. */
public record CameraInventory(
        List<Camera> cameras,
        List<List<String>> concurrentGroups) {
    public static final int MAX_CAMERAS = 32;
    public static final int MAX_PHYSICAL_CAMERAS = 16;
    public static final int MAX_FOCAL_LENGTHS = 16;
    public static final int MAX_COMMON_SIZES = 32;
    public static final int MAX_CONCURRENT_GROUPS = 16;
    public static final int MAX_CAMERAS_PER_GROUP = 8;
    public static final int MAX_ID_CHARS = 64;
    public static final int MAX_JSON_CHARS = 65_536;

    public CameraInventory {
        cameras = boundedCopy(cameras, MAX_CAMERAS, "cameras");
        List<List<String>> copiedGroups = boundedCopy(
                concurrentGroups, MAX_CONCURRENT_GROUPS, "concurrent groups");
        List<List<String>> validatedGroups = new ArrayList<>(copiedGroups.size());
        for (List<String> group : copiedGroups) {
            List<String> copied = boundedCopy(group, MAX_CAMERAS_PER_GROUP, "concurrent group");
            if (copied.size() < 2) {
                throw new IllegalArgumentException("concurrent groups require at least two cameras");
            }
            for (String id : copied) {
                validateId(id);
            }
            validatedGroups.add(copied);
        }
        concurrentGroups = List.copyOf(validatedGroups);
    }

    public String toJson() {
        StringBuilder output = new StringBuilder(Math.min(MAX_JSON_CHARS, 1024));
        output.append("{\"version\":1,\"cameras\":[");
        for (int index = 0; index < cameras.size(); index++) {
            if (index > 0) {
                output.append(',');
            }
            cameras.get(index).appendJson(output);
            requireJsonBound(output);
        }
        output.append("],\"concurrentGroups\":[");
        for (int groupIndex = 0; groupIndex < concurrentGroups.size(); groupIndex++) {
            if (groupIndex > 0) {
                output.append(',');
            }
            output.append('[');
            List<String> group = concurrentGroups.get(groupIndex);
            for (int idIndex = 0; idIndex < group.size(); idIndex++) {
                if (idIndex > 0) {
                    output.append(',');
                }
                appendJsonString(output, group.get(idIndex));
            }
            output.append(']');
            requireJsonBound(output);
        }
        output.append("]}");
        requireJsonBound(output);
        return output.toString();
    }

    public record Camera(
            String id,
            LensFacing facing,
            String hardwareLevel,
            int sensorOrientationDegrees,
            List<String> physicalIds,
            List<PhysicalLens> physicalLenses,
            List<Integer> focalLengthMilliMm,
            Integer minimumZoomRatioMilli,
            Integer maximumZoomRatioMilli,
            List<Size> commonPreviewEncoderSizes) {
        public Camera {
            validateId(id);
            facing = Objects.requireNonNull(facing, "facing");
            hardwareLevel = validateLabel(hardwareLevel, "hardware level");
            if (sensorOrientationDegrees != 0
                    && sensorOrientationDegrees != 90
                    && sensorOrientationDegrees != 180
                    && sensorOrientationDegrees != 270) {
                throw new IllegalArgumentException("invalid sensor orientation");
            }
            physicalIds = boundedCopy(
                    physicalIds, MAX_PHYSICAL_CAMERAS, "physical camera ids");
            for (String physicalId : physicalIds) {
                validateId(physicalId);
            }
            physicalLenses = boundedCopy(
                    physicalLenses, MAX_PHYSICAL_CAMERAS, "physical camera details");
            for (PhysicalLens physicalLens : physicalLenses) {
                if (!physicalIds.contains(physicalLens.id())) {
                    throw new IllegalArgumentException(
                            "physical camera detail is not declared by the logical camera");
                }
            }
            focalLengthMilliMm = boundedCopy(
                    focalLengthMilliMm, MAX_FOCAL_LENGTHS, "focal lengths");
            for (Integer focalLength : focalLengthMilliMm) {
                if (focalLength == null || focalLength <= 0 || focalLength > 1_000_000) {
                    throw new IllegalArgumentException("invalid focal length");
                }
            }
            if ((minimumZoomRatioMilli == null) != (maximumZoomRatioMilli == null)) {
                throw new IllegalArgumentException("zoom ratio bounds must be both present or absent");
            }
            if (minimumZoomRatioMilli != null
                    && (minimumZoomRatioMilli <= 0
                            || maximumZoomRatioMilli < minimumZoomRatioMilli
                            || maximumZoomRatioMilli > 1_000_000)) {
                throw new IllegalArgumentException("invalid zoom ratio range");
            }
            commonPreviewEncoderSizes = boundedCopy(
                    commonPreviewEncoderSizes, MAX_COMMON_SIZES, "common camera sizes");
        }

        private void appendJson(StringBuilder output) {
            output.append('{');
            appendField(output, "id", id);
            output.append(",\"facing\":");
            appendJsonString(output, facing.name().toLowerCase(java.util.Locale.ROOT));
            output.append(",\"hardwareLevel\":");
            appendJsonString(output, hardwareLevel);
            output.append(",\"sensorOrientationDegrees\":").append(sensorOrientationDegrees);
            output.append(",\"physicalIds\":[");
            appendStringList(output, physicalIds);
            output.append("],\"physicalLenses\":[");
            for (int index = 0; index < physicalLenses.size(); index++) {
                if (index > 0) {
                    output.append(',');
                }
                physicalLenses.get(index).appendJson(output);
            }
            output.append("],\"focalLengthMilliMm\":[");
            for (int index = 0; index < focalLengthMilliMm.size(); index++) {
                if (index > 0) {
                    output.append(',');
                }
                output.append(focalLengthMilliMm.get(index));
            }
            output.append("],\"zoomRatioMilli\":");
            if (minimumZoomRatioMilli == null) {
                output.append("null");
            } else {
                output.append('[')
                        .append(minimumZoomRatioMilli)
                        .append(',')
                        .append(maximumZoomRatioMilli)
                        .append(']');
            }
            output.append(",\"commonPreviewEncoderSizes\":[");
            for (int index = 0; index < commonPreviewEncoderSizes.size(); index++) {
                if (index > 0) {
                    output.append(',');
                }
                Size size = commonPreviewEncoderSizes.get(index);
                output.append('[').append(size.width()).append(',').append(size.height()).append(']');
            }
            output.append("]}");
        }
    }

    public record PhysicalLens(
            String id,
            List<Integer> focalLengthMilliMm,
            Integer sensorWidthMicroMm,
            Integer sensorHeightMicroMm) {
        public PhysicalLens {
            validateId(id);
            focalLengthMilliMm = boundedCopy(
                    focalLengthMilliMm, MAX_FOCAL_LENGTHS, "physical focal lengths");
            for (Integer focalLength : focalLengthMilliMm) {
                if (focalLength == null || focalLength <= 0 || focalLength > 1_000_000) {
                    throw new IllegalArgumentException("invalid physical focal length");
                }
            }
            if ((sensorWidthMicroMm == null) != (sensorHeightMicroMm == null)) {
                throw new IllegalArgumentException("physical sensor dimensions must be paired");
            }
            if (sensorWidthMicroMm != null
                    && (sensorWidthMicroMm <= 0
                            || sensorHeightMicroMm <= 0
                            || sensorWidthMicroMm > 1_000_000
                            || sensorHeightMicroMm > 1_000_000)) {
                throw new IllegalArgumentException("invalid physical sensor dimensions");
            }
        }

        private void appendJson(StringBuilder output) {
            output.append('{');
            appendField(output, "id", id);
            output.append(",\"focalLengthMilliMm\":[");
            for (int index = 0; index < focalLengthMilliMm.size(); index++) {
                if (index > 0) {
                    output.append(',');
                }
                output.append(focalLengthMilliMm.get(index));
            }
            output.append("],\"sensorSizeMicroMm\":");
            if (sensorWidthMicroMm == null) {
                output.append("null");
            } else {
                output.append('[')
                        .append(sensorWidthMicroMm)
                        .append(',')
                        .append(sensorHeightMicroMm)
                        .append(']');
            }
            output.append('}');
        }
    }

    public record Size(int width, int height) {
        public Size {
            if (width <= 0 || width > 16_384 || height <= 0 || height > 16_384) {
                throw new IllegalArgumentException("invalid camera size");
            }
        }
    }

    public enum LensFacing {
        FRONT,
        BACK,
        EXTERNAL,
        UNKNOWN
    }

    private static <T> List<T> boundedCopy(List<T> values, int maximum, String label) {
        Objects.requireNonNull(values, label);
        if (values.size() > maximum) {
            throw new IllegalArgumentException(label + " exceed bound " + maximum);
        }
        return List.copyOf(values);
    }

    private static void validateId(String id) {
        Objects.requireNonNull(id, "camera id");
        if (id.isEmpty() || id.length() > MAX_ID_CHARS) {
            throw new IllegalArgumentException("invalid camera id length");
        }
        for (int index = 0; index < id.length(); index++) {
            if (Character.isISOControl(id.charAt(index))) {
                throw new IllegalArgumentException("camera id contains a control character");
            }
        }
    }

    private static String validateLabel(String value, String label) {
        Objects.requireNonNull(value, label);
        if (value.isEmpty() || value.length() > 32) {
            throw new IllegalArgumentException("invalid " + label);
        }
        return value;
    }

    private static void appendField(StringBuilder output, String name, String value) {
        appendJsonString(output, name);
        output.append(':');
        appendJsonString(output, value);
    }

    private static void appendStringList(StringBuilder output, List<String> values) {
        for (int index = 0; index < values.size(); index++) {
            if (index > 0) {
                output.append(',');
            }
            appendJsonString(output, values.get(index));
        }
    }

    private static void appendJsonString(StringBuilder output, String value) {
        output.append('"');
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            switch (character) {
                case '"' -> output.append("\\\"");
                case '\\' -> output.append("\\\\");
                case '\b' -> output.append("\\b");
                case '\f' -> output.append("\\f");
                case '\n' -> output.append("\\n");
                case '\r' -> output.append("\\r");
                case '\t' -> output.append("\\t");
                default -> output.append(character);
            }
        }
        output.append('"');
    }

    private static void requireJsonBound(StringBuilder output) {
        if (output.length() > MAX_JSON_CHARS) {
            throw new IllegalStateException("camera inventory JSON exceeds its fixed bound");
        }
    }
}
