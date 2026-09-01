package dev.capyio.android;

import android.content.Context;
import android.content.SharedPreferences;

import java.util.UUID;

final class NodeIdentityStore {
    private static final String PREFERENCES = "capyio-node-identity-v1";
    private static final String NODE_ID = "node-id";

    private NodeIdentityStore() {}

    static String loadOrCreate(Context context) {
        SharedPreferences preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE);
        String existing = preferences.getString(NODE_ID, null);
        if (existing != null) {
            try {
                return UUID.fromString(existing).toString();
            } catch (IllegalArgumentException ignored) {
                // Replace only a malformed value owned by this app's private storage.
            }
        }
        String created = UUID.randomUUID().toString();
        if (!preferences.edit().putString(NODE_ID, created).commit()) {
            throw new IllegalStateException("cannot persist CapyIO Node identity");
        }
        return created;
    }
}
