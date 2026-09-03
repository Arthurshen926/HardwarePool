package dev.capyio.android.contract;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/** Immutable, payload-free Android-local Port DTO. */
public final class CapabilityPortSnapshot {
    private static final int MAX_FORMATS = 16;

    private final String portKey;
    private final String displayName;
    private final CapabilityPortDirection direction;
    private final String profile;
    private final List<String> formats;

    public CapabilityPortSnapshot(
            String portKey,
            String displayName,
            CapabilityPortDirection direction,
            String profile,
            List<String> formats) {
        this.portKey = CapabilitySnapshot.requireKey(portKey, "port key");
        this.displayName = CapabilitySnapshot.requireDisplayName(displayName, "Port display name");
        if (direction == null) {
            throw new IllegalArgumentException("Port direction is required");
        }
        this.direction = direction;
        this.profile = CapabilitySnapshot.requireProfile(profile);
        if (formats == null || formats.isEmpty() || formats.size() > MAX_FORMATS) {
            throw new IllegalArgumentException("Port formats are outside the supported bound");
        }
        List<String> copiedFormats = new ArrayList<>(formats.size());
        Set<String> uniqueFormats = new HashSet<>();
        for (String format : formats) {
            String validated = CapabilitySnapshot.requireKey(format, "format ID");
            if (!uniqueFormats.add(validated)) {
                throw new IllegalArgumentException("Port format IDs must be unique");
            }
            copiedFormats.add(validated);
        }
        this.formats = Collections.unmodifiableList(copiedFormats);
    }

    public String portKey() {
        return portKey;
    }

    public String displayName() {
        return displayName;
    }

    public CapabilityPortDirection direction() {
        return direction;
    }

    public String profile() {
        return profile;
    }

    public List<String> formats() {
        return formats;
    }
}
