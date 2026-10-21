package io.capyio.camera.lab;

import android.content.Context;

import io.capyio.camera.contract.AvcQualityPreset;
import io.capyio.camera.contract.CameraTransportEndpoint;
import io.capyio.camera.contract.CameraSourceSelection;
import io.capyio.camera.contract.FrameObservation;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.util.List;
import java.util.Locale;

/** Narrow public facade used by the unified Node host without exposing camera internals. */
public final class UnifiedCameraNodeFacade {
    private static final int MAX_SELECTION_KEY_LENGTH = 96;
    private static CameraSourceSelection preferredSource;
    private static AvcQualityPreset preferredQuality = AvcQualityPreset.BALANCED;
    private static List<CameraSourceSelection> availableSources = List.of();
    private static String inventoryProblemCode;
    private static CameraTransportEndpoint peerEndpoint;
    private static volatile String transportProblemCode;
    private static CameraRouteBinding routeBinding;
    private static volatile SecureCameraBinding secureBinding;

    public static synchronized boolean configureRoute(String host, String stream, long epoch, long expiry) {
        if (isRunning() || !configurePeer(host)) return false;
        secureBinding = null;
        routeBinding = new CameraRouteBinding(stream,epoch,expiry);
        return true;
    }

    /** Select only the in-process native transport; no host/port/credential crosses this seam. */
    public static synchronized boolean configureSecureRoute(
            io.capyio.camera.contract.SecureCameraTransport transport, long generation,
            String stream, long epoch, long expiry) {
        if (isRunning()) return false;
        SecureCameraBinding binding = new SecureCameraBinding(transport, generation);
        CameraRouteBinding route = new CameraRouteBinding(stream, epoch, expiry);
        if (!binding.ready() || !route.isLive()) return false;
        peerEndpoint = null;
        secureBinding = binding;
        routeBinding = route;
        return true;
    }

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

    /** Called by the Node service with the address received over authenticated control. */
    public static synchronized boolean configurePeer(String host) {
        if (host == null || host.isEmpty()) {
            peerEndpoint = null;
            return true;
        }
        try {
            CameraTransportEndpoint endpoint = CameraTransportEndpoint.fromUserInput(host);
            if (endpoint.mode() != CameraTransportEndpoint.Mode.TRUSTED_LAN) return false;
            if (isRunning() && !endpoint.equals(peerEndpoint)) return false;
            peerEndpoint = endpoint;
            transportProblemCode = null;
            return true;
        } catch (IllegalArgumentException rejected) {
            return false;
        }
    }

    public static synchronized void start(Context context) {
        if (peerEndpoint == null && secureBinding == null) {
            transportProblemCode = "CAPY.ANDROID.CAMERA_AUTHENTICATED_PEER_REQUIRED";
            return;
        }
        transportProblemCode = null;
        FrameObservation.LensFacing facing = preferredSource == null
                ? FrameObservation.LensFacing.BACK
                : mapFacing(preferredSource.facing());
        if (secureBinding != null) {
            CameraCaptureService.startSecure(context, facing, preferredSource,
                    preferredQuality, routeBinding, secureBinding);
            return;
        }
        CameraCaptureService.start(
                context,
                facing,
                preferredSource,
                preferredQuality,
                peerEndpoint, routeBinding);
    }

    public static synchronized void stop(Context context) {
        SecureCameraBinding secure = secureBinding;
        secureBinding = null;
        if (secure != null) secure.stop();
        routeBinding = null;
        transportProblemCode = null;
        CameraCaptureService.stop(context);
    }

    public static Snapshot snapshot() {
        CameraCaptureService.Snapshot source = CameraCaptureService.snapshot();
        return new Snapshot(
                transportProblemCode != null ? "failed" : switch (source.phase()) {
                    case STOPPED -> "stopped";
                    case STARTING -> "starting";
                    case STREAMING -> source.secureTransport() ? "starting" : "active";
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
                transportProblemCode != null ? transportProblemCode : source.phase() == CameraCaptureService.Phase.ERROR
                        ? "CAPY.ANDROID.CAMERA_CAPTURE_FAILED"
                        : null);
    }

    public static synchronized boolean refreshSources(Context context) {
        if (isRunning()) {
            return false;
        }
        try {
            availableSources = CameraSourceSelection.enumerate(
                    CameraInventoryCollector.collect(context));
            if (preferredSource != null && !availableSources.contains(preferredSource)) {
                preferredSource = null;
            }
            inventoryProblemCode = null;
            return true;
        } catch (Exception failure) {
            availableSources = List.of();
            preferredSource = null;
            inventoryProblemCode = "CAPY.ANDROID.CAMERA_INVENTORY_FAILED";
            return false;
        }
    }

    public static synchronized boolean selectSource(String sourceKey) {
        if (sourceKey == null || sourceKey.length() > MAX_SELECTION_KEY_LENGTH || isRunning()) {
            return false;
        }
        if (sourceKey.isEmpty()) {
            preferredSource = null;
            return true;
        }
        for (CameraSourceSelection candidate : availableSources) {
            if (candidate.key().equals(sourceKey)) {
                preferredSource = candidate;
                return true;
            }
        }
        return false;
    }

    public static synchronized boolean selectQuality(String quality) {
        if (quality == null || quality.length() > 16 || isRunning()) {
            return false;
        }
        try {
            preferredQuality = AvcQualityPreset.valueOf(quality.toUpperCase(Locale.ROOT));
            return true;
        } catch (IllegalArgumentException rejected) {
            return false;
        }
    }

    public static synchronized String settingsJson(boolean permissionReady) {
        Snapshot lifecycle = snapshot();
        try {
            JSONObject result = new JSONObject()
                    .put("schemaVersion", 1)
                    .put("state", lifecycle.state())
                    .put("generation", lifecycle.generation())
                    .put("transportStatus", lifecycle.transportStatus())
                    .put("capturedFrames", lifecycle.capturedFrames())
                    .put("encodedAccessUnits", lifecycle.encodedAccessUnits())
                    .put("sentAccessUnits", lifecycle.sentAccessUnits())
                    .put("exportDroppedAccessUnits", lifecycle.exportDroppedAccessUnits())
                    .put("permissionReady", permissionReady)
                    .put("selectedSourceKey", preferredSource == null
                            ? JSONObject.NULL : preferredSource.key())
                    .put("quality", preferredQuality.name().toLowerCase(Locale.ROOT))
                    .put("problemCode", lifecycle.problemCode() != null ? lifecycle.problemCode()
                            : inventoryProblemCode == null ? JSONObject.NULL : inventoryProblemCode);
            JSONArray sources = new JSONArray();
            for (CameraSourceSelection source : availableSources) {
                sources.put(new JSONObject()
                        .put("key", source.key())
                        .put("cameraId", source.cameraId())
                        .put("facing", source.facing().name().toLowerCase(Locale.ROOT))
                        .put("zoomRatioMilli", source.targetZoomRatioMilli() == null
                                ? JSONObject.NULL : source.targetZoomRatioMilli()));
            }
            result.put("sources", sources);
            return result.toString();
        } catch (JSONException impossible) {
            throw new IllegalStateException("Camera settings JSON construction failed", impossible);
        }
    }

    private static boolean isRunning() {
        CameraCaptureService.Phase phase = CameraCaptureService.snapshot().phase();
        return phase == CameraCaptureService.Phase.STARTING
                || phase == CameraCaptureService.Phase.STREAMING;
    }

    private static FrameObservation.LensFacing mapFacing(
            io.capyio.camera.contract.CameraInventory.LensFacing facing) {
        return switch (facing) {
            case FRONT -> FrameObservation.LensFacing.FRONT;
            case BACK -> FrameObservation.LensFacing.BACK;
            case EXTERNAL -> FrameObservation.LensFacing.EXTERNAL;
            case UNKNOWN -> FrameObservation.LensFacing.UNKNOWN;
        };
    }
}
