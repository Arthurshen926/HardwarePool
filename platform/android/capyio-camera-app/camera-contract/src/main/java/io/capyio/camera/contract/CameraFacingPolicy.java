package io.capyio.camera.contract;

import java.util.List;
import java.util.Objects;

/** Deterministic bounded policy for selecting a user-requested camera facing. */
public final class CameraFacingPolicy {
    private static final int MAX_CAMERA_CANDIDATES = 32;

    private CameraFacingPolicy() {}

    public static FrameObservation.LensFacing toggle(
            FrameObservation.LensFacing current) {
        return switch (Objects.requireNonNull(current, "current")) {
            case BACK -> FrameObservation.LensFacing.FRONT;
            case FRONT -> FrameObservation.LensFacing.BACK;
            case EXTERNAL, UNKNOWN -> FrameObservation.LensFacing.BACK;
        };
    }

    public static FrameObservation.LensFacing select(
            FrameObservation.LensFacing preferred,
            List<FrameObservation.LensFacing> available) {
        Objects.requireNonNull(preferred, "preferred");
        Objects.requireNonNull(available, "available");
        if (available.isEmpty() || available.size() > MAX_CAMERA_CANDIDATES) {
            throw new IllegalArgumentException("camera candidate count is outside the bound");
        }
        for (FrameObservation.LensFacing candidate : available) {
            Objects.requireNonNull(candidate, "candidate");
        }
        for (FrameObservation.LensFacing candidate : available) {
            if (candidate == preferred) {
                return candidate;
            }
        }
        for (FrameObservation.LensFacing candidate : available) {
            if (candidate == FrameObservation.LensFacing.BACK) {
                return candidate;
            }
        }
        return available.get(0);
    }
}
