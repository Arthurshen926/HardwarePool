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

    AndroidCameraUseModule(Context context) {
        this.context = context;
    }

    void start(AndroidNativeCatalogRuntime runtime, String peer) {
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
            use.remove("expiresAtMs");
            return use.toString();
        } catch (org.json.JSONException invalid) {
            return "{\"status\":\"idle\"}";
        }
    }

    void close() {
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
}
