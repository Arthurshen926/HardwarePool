package io.capyio.camera.contract;

import java.util.Objects;

/** Metadata observed at the Camera2 boundary; it intentionally carries no pixels. */
public record FrameObservation(
        int width,
        int height,
        long sourceTimestampNs,
        long sequence,
        int sensorOrientationDegrees,
        LensFacing lensFacing) {
    public enum LensFacing {
        FRONT,
        BACK,
        EXTERNAL,
        UNKNOWN
    }

    public FrameObservation {
        if (width <= 0 || height <= 0 || (width & 1) != 0 || (height & 1) != 0) {
            throw new IllegalArgumentException("frame dimensions must be positive and even");
        }
        if (sourceTimestampNs < 0) {
            throw new IllegalArgumentException("source timestamp must be non-negative");
        }
        if (sequence <= 0) {
            throw new IllegalArgumentException("sequence must be positive");
        }
        if (sensorOrientationDegrees != 0
                && sensorOrientationDegrees != 90
                && sensorOrientationDegrees != 180
                && sensorOrientationDegrees != 270) {
            throw new IllegalArgumentException("sensor orientation must be 0/90/180/270");
        }
        Objects.requireNonNull(lensFacing, "lensFacing");
    }
}
