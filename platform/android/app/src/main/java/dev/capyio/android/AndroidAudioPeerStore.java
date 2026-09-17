package dev.capyio.android;

import android.content.Context;

/** Private per-direction settings; missing settings retain the lab-build fallback. */
final class AndroidAudioPeerStore {
    private AndroidAudioPeerStore() {}

    static AndroidAudioPeer load(Context context, String direction) {
        String encoded = context.getSharedPreferences("capyio-audio-peers-v1", Context.MODE_PRIVATE)
                .getString(direction, null);
        if (encoded == null) return null;
        String[] fields = encoded.split(":", -1);
        if (fields.length != 3) throw new IllegalArgumentException("Invalid saved audio endpoint");
        return new AndroidAudioPeer(fields[0], Integer.parseInt(fields[1]), Integer.parseInt(fields[2]));
    }

    static void save(Context context, String direction, AndroidAudioPeer peer) {
        if (!context.getSharedPreferences("capyio-audio-peers-v1", Context.MODE_PRIVATE).edit()
                .putString(direction, peer.host() + ":" + peer.localPort() + ":" + peer.peerPort()).commit()) {
            throw new IllegalStateException("Audio settings could not be saved");
        }
    }
}
