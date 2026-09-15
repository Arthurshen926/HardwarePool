package io.capyio.camera.lab;

import android.content.Context;

import io.capyio.camera.contract.AvcQualityPreset;
import io.capyio.camera.contract.CameraTransportEndpoint;
import io.capyio.camera.contract.FrameObservation;

/** Narrow public facade used by the unified Node host without exposing camera internals. */
public final class UnifiedCameraNodeFacade {
    public record Snapshot(
            String state,
            long generation,
            int width,
            int height,
            String facing,
            String sourceKey,
            String quality,
            long capturedFrames,
            long encodedAccessUnits,
            long droppedAccessUnits,
            String transportStatus,
            long sentAccessUnits,
            long exportDroppedAccessUnits,
            String encoderStatus,
            String problemCode) {}

    private UnifiedCameraNodeFacade() {}

    public static void start(Context context) {
        CameraCaptureService.start(
                context,
                FrameObservation.LensFacing.BACK,
                null,
                AvcQualityPreset.BALANCED,
                CameraTransportEndpoint.adbReverse());
    }

    public static void stop(Context context) {
        CameraCaptureService.stop(context);
    }

    public static Snapshot snapshot() {
        CameraCaptureService.Snapshot source = CameraCaptureService.snapshot();
        return new Snapshot(
                switch (source.phase()) {
                    case STOPPED -> "stopped";
                    case STARTING -> "starting";
                    case STREAMING -> "active";
                    case ERROR -> "failed";
                },
                source.generation(),
                source.width(),
                source.height(),
                source.facing().name().toLowerCase(java.util.Locale.ROOT),
                source.sourceKey(),
                source.quality().name().toLowerCase(java.util.Locale.ROOT),
                source.capturedFrames(),
                source.encodedAccessUnits(),
                source.droppedAccessUnits(),
                source.transportStatus(),
                source.sentAccessUnits(),
                source.exportDroppedAccessUnits(),
                source.encoderStatus(),
                source.phase() == CameraCaptureService.Phase.ERROR
                        ? "CAPY.ANDROID.CAMERA_CAPTURE_FAILED"
                        : null);
    }
}
