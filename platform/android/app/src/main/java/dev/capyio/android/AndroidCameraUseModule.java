package dev.capyio.android;

import android.Manifest;
import android.content.Context;
import android.content.pm.PackageManager;
import io.capyio.camera.lab.UnifiedCameraNodeFacade;

/** Camera Route orchestration. All calls run under the owning Node Service monitor. */
final class AndroidCameraUseModule {
    private final Context context;
    private String useJson = "{\"status\":\"idle\"}";
    private String routeId;
    private long startedAt;
    private io.capyio.camera.contract.SecureCameraCaptureSession secureSession;
    private org.json.JSONObject secureLaunchConfig;
    private String secureRouteId;
    private String attemptedSecureRoute;

    AndroidCameraUseModule(Context context) {
        this.context = context;
    }

    void start(AndroidNativeCatalogRuntime runtime, String peer) {
        close();
        attemptedSecureRoute = null; // Only an explicit user Start permits a new generation.
        useJson = runtime.cameraUse("start", peer);
    }

    void stop(AndroidNativeCatalogRuntime runtime) {
        useJson = runtime.cameraUse("stop", "");
        synchronize(runtime);
    }

    String publicStateJson() {
        try {
            org.json.JSONObject use = new org.json.JSONObject(useJson);
            use.remove("config");
            use.remove("secureConfig");
            use.remove("expiresAtMs");
            return use.toString();
        } catch (org.json.JSONException invalid) {
            return "{\"status\":\"idle\"}";
        }
    }

    void close() {
        if (secureSession != null) secureSession.close();
        secureSession = null;
        secureLaunchConfig = null;
        secureRouteId = null;
        if (routeId != null) {
            UnifiedCameraNodeFacade.stop(context);
            routeId = null;
        }
    }

    void synchronize(AndroidNativeCatalogRuntime runtime) {
        try {
            useJson = runtime.cameraUse("poll", "");
            org.json.JSONObject use = new org.json.JSONObject(useJson);
            String id = use.optString("routeId");
            boolean run = use.optBoolean("shouldRun");
            if (use.optBoolean("secureTransport")) {
                synchronizeSecure(runtime, use, id, run);
                return;
            }
            if (secureSession != null) close();
            if (routeId != null && (!run || !routeId.equals(id))) {
                close();
            }
            if (!run) return;
            if (routeId == null) {
                if (context.checkSelfPermission(Manifest.permission.CAMERA)
                        != PackageManager.PERMISSION_GRANTED) {
                    useJson = runtime.cameraUse("failed", "");
                    return;
                }
                org.json.JSONObject config = use.getJSONObject("config");
                if (!UnifiedCameraNodeFacade.configureRoute(config.getString("host"),
                        config.getString("streamId"), config.getLong("epoch"),
                        use.getLong("expiresAtMs"))) {
                    useJson = runtime.cameraUse("failed", "");
                    return;
                }
                routeId = id;
                startedAt = android.os.SystemClock.elapsedRealtime();
                UnifiedCameraNodeFacade.start(context);
            }
            UnifiedCameraNodeFacade.Snapshot snapshot = UnifiedCameraNodeFacade.snapshot();
            if (snapshot.sentAccessUnits() > 0 && "active".equals(snapshot.state())
                    && !"active".equals(use.optString("status"))) {
                useJson = runtime.cameraUse("active", "");
            } else if ("failed".equals(snapshot.state())
                    || ("stopped".equals(snapshot.state())
                        && android.os.SystemClock.elapsedRealtime() - startedAt > 1000)
                    || (snapshot.sentAccessUnits() == 0
                        && android.os.SystemClock.elapsedRealtime() - startedAt > 15000)) {
                useJson = runtime.cameraUse("failed", "");
            }
        } catch (RuntimeException | org.json.JSONException failed) {
            close();
            try {
                useJson = runtime.cameraUse("failed", "");
            } catch (RuntimeException unavailable) {
                // Runtime teardown also stops capture.
            }
        }
    }

    private void synchronizeSecure(AndroidNativeCatalogRuntime runtime,
            org.json.JSONObject use, String id, boolean run) throws org.json.JSONException {
        long expiry = use.optLong("expiresAtMs", 0);
        boolean pending = use.optBoolean("secureCapturePending");
        if (id.isEmpty() || expiry <= System.currentTimeMillis() || (!pending && !run)) {
            close();
            return;
        }
        if (secureRouteId != null && !secureRouteId.equals(id)) close();
        secureLaunchConfig = use.optJSONObject("secureConfig");
        if (secureSession == null) {
            if (id.equals(attemptedSecureRoute)) return;
            attemptedSecureRoute = id;
            secureRouteId = id;
            secureSession = new io.capyio.camera.contract.SecureCameraCaptureSession(
                    runtime, id, expiry,
                    new io.capyio.camera.contract.SecureCameraCaptureSession.Capture() {
                        @Override public boolean permissionGranted() {
                            return context.checkSelfPermission(Manifest.permission.CAMERA)
                                    == PackageManager.PERMISSION_GRANTED;
                        }
                        @Override public boolean start(long generation) {
                            try {
                                org.json.JSONObject config = secureLaunchConfig;
                                if (config == null || !UnifiedCameraNodeFacade.configureSecureRoute(
                                        runtime, generation, config.getString("streamId"),
                                        config.getLong("epoch"), expiry)) return false;
                                routeId = id;
                                startedAt = android.os.SystemClock.elapsedRealtime();
                                UnifiedCameraNodeFacade.start(context);
                                return true;
                            } catch (org.json.JSONException invalid) {
                                return false;
                            }
                        }
                        @Override public void stop() {
                            UnifiedCameraNodeFacade.stop(context);
                            routeId = null;
                        }
                    });
            secureSession.start();
        }
        secureSession.poll(run);
        if (secureSession.terminal()) {
            close();
            useJson = runtime.cameraUse("failed", "");
            return;
        }
        if (!secureSession.capturing()) return;
        UnifiedCameraNodeFacade.Snapshot snapshot = UnifiedCameraNodeFacade.snapshot();
        // Never synthesize receiver Active from local capture, TLS Ready or record enqueue.
        // The authenticated native control snapshot is the sole source of secure status.
        if ("failed".equals(snapshot.state())
                || ("stopped".equals(snapshot.state())
                    && android.os.SystemClock.elapsedRealtime() - startedAt > 1000)
                || (snapshot.encodedAccessUnits() == 0
                    && android.os.SystemClock.elapsedRealtime() - startedAt > 15000)) {
            close();
            useJson = runtime.cameraUse("failed", "");
        }
    }
}
