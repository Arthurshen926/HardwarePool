package dev.capyio.android;

import android.content.SharedPreferences;
import android.os.Build;
import dev.capyio.android.contract.CapabilityCatalogSnapshot;
import java.util.HashSet;
import java.util.Set;
import java.util.function.Consumer;

/** Owns publication selection, persistence and catalog projection. */
final class AndroidCatalogModule {
    private final AndroidSharingSelection selection = new AndroidSharingSelection();
    private final SharedPreferences preferences;

    AndroidCatalogModule(SharedPreferences preferences) {
        this.preferences = preferences;
    }

    void restore(CapabilityCatalogSnapshot catalog) {
        // Preserve the previously automatic speaker publication once, then honor opt-outs.
        if (!preferences.getBoolean("speaker-publication-migrated", false)) {
            Set<String> migrated = new HashSet<>(
                    preferences.getStringSet("shared-capabilities", Set.of()));
            migrated.add(catalog.nodeId() + ":android.speaker");
            preferences.edit().putStringSet("shared-capabilities", migrated)
                    .putBoolean("speaker-publication-migrated", true).apply();
        }
        for (String id : preferences.getStringSet("shared-capabilities", Set.of())) {
            selection.update(catalog, id, true);
        }
    }

    Set<String> selectedIds() {
        return selection.snapshot();
    }

    CapabilityCatalogSnapshot publication(CapabilityCatalogSnapshot catalog) {
        return selection.publicationCatalog(catalog);
    }

    String publicationJson(CapabilityCatalogSnapshot catalog) {
        return encode(publication(catalog));
    }

    static String encode(CapabilityCatalogSnapshot publication) {
        return AndroidCoreCatalogJson.encode(
                publication, Build.VERSION.RELEASE, BuildConfig.VERSION_NAME);
    }

    boolean select(CapabilityCatalogSnapshot catalog, String capabilityId, boolean selected,
            Consumer<CapabilityCatalogSnapshot> beforeCommit) {
        boolean accepted = selection.update(catalog, capabilityId, selected, beforeCommit);
        if (accepted) {
            preferences.edit().putStringSet("shared-capabilities", new HashSet<>(selectedIds()))
                    .apply();
        }
        return accepted;
    }
}
