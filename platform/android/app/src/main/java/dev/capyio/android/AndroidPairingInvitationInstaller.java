package dev.capyio.android;

import android.content.Context;

import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Imports a target-bound invitation without exposing credentials to WebView JavaScript. */
final class AndroidPairingInvitationInstaller {
    private static final int MAX_INVITATION_BYTES = 131_100;

    private AndroidPairingInvitationInstaller() {}

    static AndroidPairingImportPlan prepare(Context context, InputStream input) throws IOException {
        String localNodeId = NodeIdentityStore.loadOrCreate(context);
        byte[] invitation = readBounded(input);
        byte[] credential = null;
        try {
            byte[] directory = AndroidNativePairingDirectory.invitationDirectory(
                    localNodeId, invitation);
            credential = AndroidNativePairingDirectory.invitationCredential(
                    localNodeId, invitation);
            AndroidPairingDirectoryStore directoryStore =
                    new AndroidPairingDirectoryStore(context, localNodeId);
            JSONArray offeredPeers = json(directory).getJSONArray("peers");
            String[] handles = directoryStore.activeCredentialHandles(directory);
            if (handles.length != 1 || offeredPeers.length() != 1) {
                throw new IllegalArgumentException(
                        "invitation must contain exactly one active peer");
            }
            AndroidNativeControlCredential.requireValid(credential);
            byte[] existing = directoryStore.load();
            JSONObject incomingRecord = offeredPeers.getJSONObject(0);
            AndroidPairingImportPlan.Peer incoming = peer(incomingRecord);
            JSONArray existingRecords = json(existing).getJSONArray("peers");
            ArrayList<AndroidPairingImportPlan.Peer> existingPeers = new ArrayList<>();
            ArrayList<AndroidPairingImportPlan.Entry<JSONObject>> entries = new ArrayList<>();
            boolean referenced = false;
            for (int index = 0; index < existingRecords.length(); index++) {
                JSONObject record = existingRecords.getJSONObject(index);
                AndroidPairingImportPlan.Peer metadata = peer(record);
                existingPeers.add(metadata);
                entries.add(new AndroidPairingImportPlan.Entry<>(metadata, record));
                if (handles[0].equals(record.getString("credentialHandle"))) referenced = true;
            }
            AndroidPairingImportPlan.Peer previous =
                    AndroidPairingImportPlan.validateReplacement(existingPeers, incoming);
            JSONArray merged = new JSONArray(AndroidPairingImportPlan.mergedRecords(entries,
                    new AndroidPairingImportPlan.Entry<>(incoming, incomingRecord)));
            byte[] mergedDirectory = AndroidNativePairingDirectory.canonicalize(localNodeId,
                    new JSONObject().put("schemaVersion", 1).put("peers", merged)
                            .toString().getBytes(StandardCharsets.UTF_8));
            return new AndroidPairingImportPlan(incoming, previous, existing, mergedDirectory,
                    handles[0], credential, referenced);
        } catch (JSONException malformed) {
            throw new IOException("pairing invitation metadata is invalid", malformed);
        } finally {
            Arrays.fill(invitation, (byte) 0);
            if (credential != null) Arrays.fill(credential, (byte) 0);
        }
    }

    static void confirm(Context context, AndroidPairingImportPlan plan) throws IOException {
        String localNodeId = NodeIdentityStore.loadOrCreate(context);
        AndroidPairingDirectoryStore directory = new AndroidPairingDirectoryStore(context, localNodeId);
        AndroidCredentialStore credentials = new AndroidCredentialStore(context);
        plan.commit(new AndroidPairingImportPlan.Store() {
            @Override public byte[] loadDirectory() throws IOException { return directory.load(); }
            @Override public byte[] loadCredential(String handle) throws IOException {
                try { return credentials.load(handle); }
                catch (AndroidCredentialStore.CredentialStoreException error) {
                    throw new IOException("existing pairing credential is unavailable", error);
                }
            }
            @Override public void putCredential(String handle, byte[] value) throws IOException {
                try { credentials.put(handle, value); }
                catch (AndroidCredentialStore.CredentialStoreException error) {
                    throw new IOException("pairing credential could not be stored", error);
                }
            }
            @Override public void commitDirectory(byte[] value) throws IOException { directory.commit(value); }
            @Override public void removeCredential(String handle) throws IOException {
                try { credentials.remove(handle); }
                catch (AndroidCredentialStore.CredentialStoreException error) {
                    throw new IOException("new pairing credential rollback failed", error);
                }
            }
        });
    }

    private static JSONObject json(byte[] bytes) throws JSONException {
        return new JSONObject(new String(bytes, StandardCharsets.UTF_8));
    }

    private static AndroidPairingImportPlan.Peer peer(JSONObject record) throws JSONException {
        return new AndroidPairingImportPlan.Peer(record.getString("nodeId"),
                record.getString("displayName"), record.getLong("pairedAtMs"),
                "active".equals(record.getJSONObject("pairing").getString("state")));
    }

    private static byte[] readBounded(InputStream input) throws IOException {
        if (input == null) throw new IOException("pairing invitation is unavailable");
        byte[] buffer = new byte[MAX_INVITATION_BYTES + 1];
        int total = 0;
        try {
            while (total <= MAX_INVITATION_BYTES) {
                int read = input.read(buffer, total, buffer.length - total);
                if (read == -1) return Arrays.copyOf(buffer, total);
                if (read == 0) {
                    int value = input.read();
                    if (value == -1) return Arrays.copyOf(buffer, total);
                    buffer[total++] = (byte) value;
                } else {
                    total += read;
                }
            }
            throw new IOException("pairing invitation exceeds its bound");
        } finally {
            Arrays.fill(buffer, (byte) 0);
        }
    }
}
