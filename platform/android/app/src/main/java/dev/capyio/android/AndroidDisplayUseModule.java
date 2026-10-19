package dev.capyio.android;

import android.content.Context;
import dev.capyio.display.app.DisplayReceiverPresentation;
import dev.capyio.display.app.NodeDisplaySession;
import java.util.Arrays;
import org.json.JSONException;
import org.json.JSONObject;

/** Display orchestration; commands and callbacks share the Node Service monitor. */
final class AndroidDisplayUseModule {
    private final Context context;
    private final Object monitor;
    private String useJson = "{\"status\":\"idle\"}";
    private String operationId;
    private boolean receiverOpened;
    private boolean closed;

    AndroidDisplayUseModule(Context context, Object monitor) {
        this.context = context;
        this.monitor = monitor;
    }

    String publicStateJson() {
        try {
            return "idle".equals(new JSONObject(useJson).optString("status")) ? "null" : useJson;
        } catch (JSONException invalid) {
            return "null";
        }
    }

    void start(AndroidNativeCatalogRuntime runtime, String peer, int width, int height,
            int refresh, int bitrate, boolean automatic) throws JSONException {
        String request = new JSONObject().put("peerNodeId", peer)
                .put("width", width).put("height", height).put("refreshMillihertz", refresh)
                .put("bitrateKbps", bitrate).put("automatic", automatic).toString();
        useJson = runtime.displayUse("start", request);
    }

    void stop(AndroidNativeCatalogRuntime runtime) {
        useJson = runtime.displayUse("stop", "");
        stopReceiver();
    }

    void close() {
        closed = true;
        stopReceiver();
    }

    private void stopReceiver() {
        operationId = null;
        NodeDisplaySession.stop();
    }

    void synchronize(AndroidNativeCatalogRuntime runtime) {
        try {
            useJson = runtime.displayUse("poll", "");
            JSONObject state = new JSONObject(useJson);
            String phase = state.optString("status");
            if (state.optBoolean("probe") && !state.optString("routeId").isEmpty()
                    && ("awaiting_consent".equals(phase) || "starting".equals(phase))) {
                JSONObject selected = DisplayQualitySelection.choose(context, state);
                useJson = runtime.displayUse(selected != null ? "supported" : "failed",
                        selected != null ? selected.toString() : "requested_display_mode_unsupported");
            }
            if (state.optBoolean("hasLaunch") && operationId == null) {
                openReceiver(runtime, state.getString("operationId"));
            }
            if (operationId != null && state.optBoolean("ready") && !receiverOpened) {
                NodeDisplaySession.begin(context, operationId);
                receiverOpened = true;
            }
            if (operationId != null && ("idle".equals(phase) || "failed".equals(phase))) {
                stopReceiver();
            }
        } catch (RuntimeException | JSONException failure) {
            stopReceiver();
            try {
                useJson = runtime.displayUse("failed", "display_receiver_failed");
            } catch (RuntimeException unavailable) {
                // Runtime teardown also closes the receiver.
            }
        }
    }

    private void openReceiver(AndroidNativeCatalogRuntime runtime, String operation) {
        byte[] record = runtime.displayRecord(null);
        try {
            operationId = operation;
            receiverOpened = false;
            NodeDisplaySession.open(context, operation, record, new NodeDisplaySession.Events() {
                @Override
                public void acknowledged(byte[] acknowledgement) {
                    synchronized (monitor) {
                        try {
                            runtime.displayRecord(acknowledgement);
                        } finally {
                            Arrays.fill(acknowledgement, (byte) 0);
                        }
                    }
                }

                @Override
                public void presented(long frames) {
                    synchronized (monitor) {
                        if (!closed && operation.equals(operationId)) {
                            useJson = runtime.displayUse("presented", Long.toString(frames));
                        }
                    }
                }

                @Override
                public void stopped(String failure) {
                    synchronized (monitor) {
                        if (!closed && operation.equals(operationId)) {
                            operationId = null;
                            useJson = runtime.displayUse(failure.isEmpty() ? "stop" : "failed",
                                    DisplayReceiverPresentation.failure(failure));
                        }
                    }
                }
            });
        } finally {
            Arrays.fill(record, (byte) 0);
        }
    }
}
