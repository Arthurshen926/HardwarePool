package dev.capyio.android.contract;

import java.util.List;

/** Immutable Android-local catalog ready for a UI host boundary. */
public final class CapabilityCatalogSnapshot {
    public static final int SCHEMA_VERSION = 1;

    private final String nodeId;
    private final int moduleCount;
    private final List<CapabilitySnapshot> capabilities;

    CapabilityCatalogSnapshot(
            String nodeId,
            int moduleCount,
            List<CapabilitySnapshot> capabilities) {
        this.nodeId = nodeId;
        this.moduleCount = moduleCount;
        this.capabilities = List.copyOf(capabilities);
    }

    public int schemaVersion() {
        return SCHEMA_VERSION;
    }

    public String nodeId() {
        return nodeId;
    }

    public int moduleCount() {
        return moduleCount;
    }

    public List<CapabilitySnapshot> capabilities() {
        return capabilities;
    }
}
