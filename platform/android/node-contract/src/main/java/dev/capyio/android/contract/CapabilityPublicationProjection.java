package dev.capyio.android.contract;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;

/** Pure Android-local projection used before an authenticated host encodes a catalog. */
public final class CapabilityPublicationProjection {
    private CapabilityPublicationProjection() {}

    public static CapabilityCatalogSnapshot project(
            CapabilityCatalogSnapshot catalog,
            Set<String> selectedSourceCapabilityIds) {
        if (catalog == null || selectedSourceCapabilityIds == null) {
            throw new IllegalArgumentException("catalog and publication selection are required");
        }
        for (String capabilityId : selectedSourceCapabilityIds) {
            CapabilitySnapshot capability = find(catalog, capabilityId);
            if (capability == null) {
                throw new IllegalArgumentException("publication Capability is not in the local catalog");
            }
            if (!hasSource(capability)) {
                throw new IllegalArgumentException("publication Capability has no Source Port");
            }
            if (!capability.adapterAttached()
                    || capability.hardwarePresence() != HardwarePresence.PRESENT
                    || capability.availability() == CapabilityAvailability.OFFLINE) {
                throw new IllegalArgumentException("publication Source is not currently eligible");
            }
        }

        List<CapabilitySnapshot> published = new ArrayList<>();
        for (CapabilitySnapshot capability : catalog.capabilities()) {
            String capabilityId = catalog.nodeId() + ":" + capability.capabilityKey();
            if (!hasSource(capability) || selectedSourceCapabilityIds.contains(capabilityId)) {
                published.add(capability);
            }
        }
        return new CapabilityCatalogSnapshot(catalog.nodeId(), catalog.moduleCount(), published);
    }

    private static boolean hasSource(CapabilitySnapshot capability) {
        return capability.ports().stream().anyMatch(
                port -> port.direction() == CapabilityPortDirection.SOURCE);
    }

    private static CapabilitySnapshot find(
            CapabilityCatalogSnapshot catalog,
            String capabilityId) {
        if (capabilityId == null) {
            return null;
        }
        for (CapabilitySnapshot capability : catalog.capabilities()) {
            if ((catalog.nodeId() + ":" + capability.capabilityKey()).equals(capabilityId)) {
                return capability;
            }
        }
        return null;
    }
}
