package dev.capyio.android;

import java.util.function.Consumer;

/** Controller Route orchestration, invoked under the Node Service monitor. */
final class AndroidControllerUseModule {
    private final ControllerRouteAdapter adapter;
    private final Consumer<String> startInput;
    private final Runnable stopSources;
    private final Runnable notifyState;
    private String routeId = "";
    private String useJson = "null";
    private long startedAt;
    private String diagnosticState = "";

    AndroidControllerUseModule(ControllerRouteAdapter adapter, Consumer<String> startInput,
            Runnable stopSources, Runnable notifyState) {
        this.adapter = adapter;
        this.startInput = startInput;
        this.stopSources = stopSources;
        this.notifyState = notifyState;
    }

    boolean ownsRoute() {
        return !routeId.isEmpty();
    }

    void start(AndroidNativeCatalogRuntime runtime, String peer, String mode) {
        useJson = runtime.controllerUse("start", peer, mode, "", false);
    }

    void stop(AndroidNativeCatalogRuntime runtime) {
        useJson = runtime.controllerUse("stop", "", "", "", false);
        synchronize(runtime);
    }

    private void stopInput() {
        stopSources.run();
        adapter.stop();
    }

    String publicStateJson() {
        try {
            org.json.JSONObject state = new org.json.JSONObject(useJson);
            state.remove("receiver"); // Transport credential never crosses the WebView boundary.
            String routeState = state.optString("state");
            state.put("status", switch (routeState) {
                case "draft" -> "awaiting_consent";
                case "prepared", "starting" -> "starting";
                case "active" -> state.optBoolean("shouldRun") ? "active" : "failed";
                case "stopping" -> "stopping";
                case "stopped" -> "idle";
                default -> "failed";
            });
            return state.toString();
        } catch (org.json.JSONException unavailable) { return "null"; }
    }

    void synchronize(AndroidNativeCatalogRuntime runtime) {
        try {
            String current = runtime.controllerUse("poll", "", "", routeId,
                    adapter.hasSentFrame());
            useJson = current;
            org.json.JSONObject use = "null".equals(current) ? null : new org.json.JSONObject(current);
            String diagnostic = use == null ? "idle" : use.optString("routeId") + " "
                    + use.optString("state") + " run=" + use.optBoolean("shouldRun")
                    + " " + use.optString("error");
            if (!diagnostic.equals(diagnosticState)) {
                diagnosticState = diagnostic;
                android.util.Log.i("CapyIOController", diagnostic);
            }
            if (use == null || !use.optBoolean("shouldRun")) {
                if (!routeId.isEmpty()) {
                    routeId = "";
                    stopInput();
                }
                return;
            }
            String route = use.getString("routeId");
            if (route.equals(routeId)) {
                if (!adapter.hasSentFrame()
                        && ("active".equals(use.optString("state"))
                        || android.os.SystemClock.elapsedRealtime() - startedAt > 10_000)) {
                    useJson = runtime.controllerUse("failed", "", "", route, false);
                    routeId = "";
                    stopInput();
                }
                return;
            }
            stopInput();
            org.json.JSONObject endpoint = use.getJSONObject("receiver");
            if (!adapter.configure(endpoint.getString("host"), endpoint.getInt("port"), endpoint.getString("token"))) return;
            routeId = route;
            startedAt = android.os.SystemClock.elapsedRealtime();
            startInput.accept(use.getString("mode"));
            notifyState.run();
        } catch (RuntimeException | org.json.JSONException failure) {
            android.util.Log.w("CapyIOController", "Service controller handoff failed: " + failure.getClass().getSimpleName());
            if (!routeId.isEmpty()) {
                routeId = "";
                stopInput();
            }
        }
    }

}
