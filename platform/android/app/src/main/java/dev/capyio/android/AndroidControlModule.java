package dev.capyio.android;

import android.content.Context;

/** Owns saved pairing state and its native control listener, on the Service lifecycle. */
final class AndroidControlModule {
    private final Context context;
    private final String nodeId;
    private final AndroidPairingDirectoryStore pairingDirectoryStore;
    private AndroidNativeControlListener controlListener;
    private byte[] pairedDirectoryJson;
    private String pairingStoreState = "not_configured";
    private int pairedPeerCount;

    AndroidControlModule(Context context, String nodeId) {
        this.context = context;
        this.nodeId = nodeId;
        pairingDirectoryStore = new AndroidPairingDirectoryStore(context, nodeId);
    }

    String state() { return pairingStoreState; }
    int peerCount() { return pairedPeerCount; }
    byte[] directory() { return pairedDirectoryJson; }
    AndroidNativeControlListener listener() { return controlListener; }

    byte[] loadCredentials() {
        byte[] pendingControlCredential = null;
        try {
            pairedDirectoryJson = pairingDirectoryStore.load();
            pairedPeerCount = pairingDirectoryStore.peerCount(pairedDirectoryJson);
            AndroidCredentialStore credentialStore = new AndroidCredentialStore(context);
            String[] activeHandles = pairingDirectoryStore.activeCredentialHandles(
                    pairedDirectoryJson);
            for (String handle : activeHandles) {
                byte[] credential = credentialStore.load(handle);
                if (activeHandles.length == 1) {
                    pendingControlCredential = credential;
                } else {
                    java.util.Arrays.fill(credential, (byte) 0);
                }
            }
            pairingStoreState = "ready";
        } catch (java.io.IOException
                | AndroidCredentialStore.CredentialStoreException
                | RuntimeException unavailable) {
            if (pendingControlCredential != null) {
                java.util.Arrays.fill(pendingControlCredential, (byte) 0);
                pendingControlCredential = null;
            }
            pairedDirectoryJson = null;
            pairedPeerCount = 0;
            pairingStoreState = "failed";
        }
        return pendingControlCredential;
    }

    void attach(AndroidNativeCatalogRuntime runtime, byte[] pendingControlCredential) {
        if (pendingControlCredential != null && pairedDirectoryJson != null) {
            try {
                controlListener = new AndroidNativeControlListener(
                        runtime, nodeId, pairedDirectoryJson, pendingControlCredential);
            } catch (RuntimeException unavailable) {
                pairingStoreState = "failed";
                pairedPeerCount = 0;
                pairedDirectoryJson = null;
            } finally {
                java.util.Arrays.fill(pendingControlCredential, (byte) 0);
            }
        }
    }

    void stopListener() {
        if (controlListener != null) {
            controlListener.close();
            controlListener = null;
        }
    }

    void clearState() {
        pairedDirectoryJson = null;
        pairedPeerCount = 0;
        pairingStoreState = "not_configured";
    }

    boolean connect(String host, int port) {
        if (controlListener == null || pairedDirectoryJson == null
                || !"ready".equals(pairingStoreState)) return false;
        byte[] credential = null;
        try {
            String[] handles = pairingDirectoryStore.activeCredentialHandles(
                    pairedDirectoryJson);
            if (handles.length != 1) return false;
            credential = new AndroidCredentialStore(context).load(handles[0]);
            return controlListener.requestDial(
                    host, port, pairedDirectoryJson, credential);
        } catch (AndroidCredentialStore.CredentialStoreException | RuntimeException error) {
            return false;
        } finally {
            if (credential != null) java.util.Arrays.fill(credential, (byte) 0);
        }
    }

    boolean reload(AndroidNativeCatalogRuntime runtime) {
        stopListener();
        byte[] credential = null;
        try {
            byte[] directory = pairingDirectoryStore.load();
            String[] activeHandles = pairingDirectoryStore.activeCredentialHandles(directory);
            if (activeHandles.length > 1) {
                throw new IllegalStateException("Android alpha supports one active control peer");
            }
            if (activeHandles.length == 1) {
                credential = new AndroidCredentialStore(context).load(activeHandles[0]);
                controlListener = new AndroidNativeControlListener(
                        runtime, nodeId, directory, credential);
            }
            pairedDirectoryJson = directory;
            pairedPeerCount = pairingDirectoryStore.peerCount(directory);
            pairingStoreState = "ready";
            return true;
        } catch (java.io.IOException
                | AndroidCredentialStore.CredentialStoreException
                | RuntimeException unavailable) {
            pairedDirectoryJson = null;
            pairedPeerCount = 0;
            pairingStoreState = "failed";
            return false;
        } finally {
            if (credential != null) java.util.Arrays.fill(credential, (byte) 0);
        }
    }

    String peerMetadataJson() {
        if (!"ready".equals(pairingStoreState) || pairedDirectoryJson == null) return "[]";
        try {
            org.json.JSONArray peers = new org.json.JSONObject(
                    new String(pairedDirectoryJson, java.nio.charset.StandardCharsets.UTF_8))
                    .getJSONArray("peers");
            org.json.JSONArray projected = new org.json.JSONArray();
            if (peers.length() > 64) return "[]";
            for (int index = 0; index < peers.length(); index++) {
                org.json.JSONObject peer = peers.getJSONObject(index);
                projected.put(new org.json.JSONObject()
                        .put("nodeId", peer.getString("nodeId"))
                        .put("displayName", peer.getString("displayName"))
                        .put("pairedAtMs", peer.getLong("pairedAtMs"))
                        .put("revoked", !"active".equals(
                                peer.getJSONObject("pairing").getString("state"))));
            }
            return projected.toString();
        } catch (org.json.JSONException invalid) {
            return "[]";
        }
    }

}
