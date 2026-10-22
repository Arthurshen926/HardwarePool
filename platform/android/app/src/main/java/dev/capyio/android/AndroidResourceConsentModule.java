package dev.capyio.android;

import android.content.Context;

/** Resource-scoped remembered consent; invoked under the Node Service monitor. */
final class AndroidResourceConsentModule {
    private static final String TOUCHPAD_TRUST_PREFERENCES = "capyio_touchpad_trust_v1";
    private final Context context;
    private final java.util.function.BooleanSupplier secureSpeaker;

    AndroidResourceConsentModule(Context context, java.util.function.BooleanSupplier secureSpeaker) {
        this.secureSpeaker = secureSpeaker;
        this.context = context;
    }

    boolean resolve(AndroidNativeCatalogRuntime runtime, String pairingStoreState,
            byte[] pairedDirectoryJson, String routeId, boolean allow, boolean remember) {
        if (runtime == null) return false;
        try {
            org.json.JSONArray requests = new org.json.JSONArray(runtime.touchpadRequestsJson());
            for (int index = 0; index < requests.length(); index++) {
                org.json.JSONObject request = requests.getJSONObject(index);
                if (!routeId.equals(request.getString("routeId"))) continue;
                if (AndroidSecureSpeakerConsent.requiresFresh(request.optString("resourceKind"), request.optString("profileId"), secureSpeaker.getAsBoolean())) remember = false;
                String peer = request.getString("peerNodeId");
                long generation = activePairingGeneration(peer, pairingStoreState, pairedDirectoryJson);
                if (remember && generation < 0) return false;
                if (!runtime.resolveTouchpadRequest(routeId, allow)) return false;
                android.content.SharedPreferences.Editor edit = context.getSharedPreferences(
                        TOUCHPAD_TRUST_PREFERENCES, Context.MODE_PRIVATE).edit();
                String key = resourceTrustKey(request);
                if (allow && remember) edit.putLong(key, generation);
                else if (!allow) edit.remove(key);
                // Migration must not let an old touchpad choice revive a denial.
                if ("touchpad".equals(request.optString("resourceKind"))) edit.remove(peer);
                edit.apply();
                return true;
            }
        } catch (org.json.JSONException unavailable) {
            return false;
        }
        return false;
    }

    void clear() {
        context.getSharedPreferences(TOUCHPAD_TRUST_PREFERENCES, Context.MODE_PRIVATE)
                .edit().clear().apply();
    }

    private static long activePairingGeneration(String peerId, String pairingStoreState, byte[] pairedDirectoryJson) throws org.json.JSONException {
        if (!"ready".equals(pairingStoreState) || pairedDirectoryJson == null) return -1;
        org.json.JSONArray peers = new org.json.JSONObject(new String(
                pairedDirectoryJson, java.nio.charset.StandardCharsets.UTF_8)).getJSONArray("peers");
        for (int index = 0; index < peers.length(); index++) {
            org.json.JSONObject peer = peers.getJSONObject(index);
            if (peerId.equals(peer.getString("nodeId"))
                    && "active".equals(peer.getJSONObject("pairing").getString("state"))) {
                return peer.getLong("pairedAtMs");
            }
        }
        return -1;
    }

    void approveRemembered(AndroidNativeCatalogRuntime runtime, String pairingStoreState, byte[] pairedDirectoryJson) {
        try {
            org.json.JSONArray requests = new org.json.JSONArray(runtime.touchpadRequestsJson());
            android.content.SharedPreferences trusted = context.getSharedPreferences(
                    TOUCHPAD_TRUST_PREFERENCES, Context.MODE_PRIVATE);
            for (int index = 0; index < requests.length(); index++) {
                org.json.JSONObject request = requests.getJSONObject(index);
                if (AndroidSecureSpeakerConsent.requiresFresh(request.optString("resourceKind"), request.optString("profileId"), secureSpeaker.getAsBoolean())) continue;
                String peer = request.getString("peerNodeId");
                long generation = activePairingGeneration(peer, pairingStoreState, pairedDirectoryJson);
                String key = resourceTrustKey(request);
                // Legacy peer-only grants applied only to touchpad, never actuators.
                if (ResourceConsentMemory.allows(request.optString("resourceKind"), generation,
                        trusted.getLong(key, -1), trusted.getLong(peer, -1))) {
                    runtime.resolveTouchpadRequest(request.getString("routeId"), true);
                }
            }
        } catch (org.json.JSONException unavailable) {
            // Keep the normal visible approval request when metadata is unavailable.
        }
    }

    private static String resourceTrustKey(org.json.JSONObject request) throws org.json.JSONException {
        return request.getString("peerNodeId") + ":" + request.getString("capabilityId");
    }

}
