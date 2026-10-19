package dev.capyio.android;

import android.content.Context;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import org.json.JSONArray;
import org.json.JSONObject;

/** Public pending metadata plus the existing Keystore-protected credential. */
final class AndroidEnrollmentJournal {
    private final String id;
    private final AndroidPairingDirectoryStore live;
    private final AndroidPairingDirectoryStore pending;
    private final AndroidCredentialStore credentials;

    AndroidEnrollmentJournal(Context context) {
        id = NodeIdentityStore.loadOrCreate(context);
        live = new AndroidPairingDirectoryStore(context, id);
        pending = new AndroidPairingDirectoryStore(context.getNoBackupFilesDir(), id, true);
        credentials = new AndroidCredentialStore(context);
    }
    private static JSONObject json(byte[] bytes) throws Exception {
        return new JSONObject(new String(bytes, StandardCharsets.UTF_8));
    }
    boolean present() throws Exception { return pending.peerCount(pending.load()) != 0; }

    String summary() throws Exception {
        JSONArray peers = json(pending.load()).getJSONArray("peers");
        if (peers.length() == 0) return null;
        if (peers.length() != 1) throw new IllegalStateException("恢复记录的节点数量无效");
        JSONObject peer = peers.getJSONObject(0);
        return new JSONObject().put("phase", "recovery_pending")
            .put("peerNodeId", peer.getString("nodeId")).put("peerName", peer.getString("displayName"))
            .put("message", "发现双方确认后保留的本机恢复记录。可使用原证书继续，不会覆盖不同的已有配对。").toString();
    }

    void stage(byte[] invitation) throws Exception {
        byte[] offered = AndroidNativePairingDirectory.invitationDirectory(id, invitation);
        byte[] credential = AndroidNativePairingDirectory.invitationCredential(id, invitation);
        try {
            String[] handles = pending.activeCredentialHandles(offered);
            if (handles.length != 1 || pending.peerCount(offered) != 1) throw new IllegalStateException("无效的待恢复配对");
            if (present() && !Arrays.equals(pending.load(), offered)) throw new IllegalStateException("已有不同的配对恢复记录，未覆盖");
            merge(offered); // Reject a changed live peer before writing anything.
            if (credentials.contains(handles[0])) {
                byte[] stored = credentials.load(handles[0]);
                try {
                    if (!java.security.MessageDigest.isEqual(stored, credential)) throw new IllegalStateException("恢复凭据不一致，未覆盖");
                } finally { Arrays.fill(stored, (byte) 0); }
            } else { credentials.put(handles[0], credential); }
            pending.commit(offered);
        } finally { Arrays.fill(credential, (byte) 0); }
    }

    void commit() throws Exception {
        byte[] offered = pending.load();
        String[] handles = pending.activeCredentialHandles(offered);
        if (handles.length == 0) return; // Idempotent after the completed journal is cleared.
        if (handles.length != 1 || pending.peerCount(offered) != 1) throw new IllegalStateException("无效的配对恢复记录");
        byte[] credential = credentials.load(handles[0]);
        Arrays.fill(credential, (byte) 0);
        live.commit(merge(offered));
        pending.commit("{\"schemaVersion\":1,\"peers\":[]}".getBytes(StandardCharsets.UTF_8));
    }

    private byte[] merge(byte[] offered) throws Exception {
        JSONArray incomingPeers = json(offered).getJSONArray("peers");
        if (incomingPeers.length() != 1) throw new IllegalStateException("无效的恢复节点数量");
        JSONObject incoming = incomingPeers.getJSONObject(0);
        JSONArray records = json(live.load()).getJSONArray("peers");
        JSONArray merged = new JSONArray();
        boolean found = false;
        for (int i = 0; i < records.length(); i++) {
            JSONObject record = records.getJSONObject(i);
            if (record.getString("nodeId").equals(incoming.getString("nodeId"))) {
                if (!record.toString().equals(incoming.toString())) throw new IllegalStateException("现有配对与恢复记录不同，未覆盖证书");
                found = true;
            } else if (record.getJSONObject("pairing").getString("state").equals("active")) {
                throw new IllegalStateException("已有其他活动配对，未替换");
            }
            merged.put(record);
        }
        if (!found) merged.put(incoming);
        return AndroidNativePairingDirectory.canonicalize(id,
            new JSONObject().put("schemaVersion", 1).put("peers", merged).toString().getBytes(StandardCharsets.UTF_8));
    }
}
