package dev.capyio.android;

import dev.capyio.android.contract.CapabilityCatalogSnapshot;
import dev.capyio.android.contract.CapabilityPortDirection;
import dev.capyio.android.contract.CapabilitySnapshot;
import dev.capyio.android.contract.HardwarePresence;

import java.util.LinkedHashSet;
import java.util.Set;

/** Service-owned, session-local publication intent for Android Source Ports. */
final class AndroidSharingSelection {
    private static final int MAX_CAPABILITY_ID_LENGTH = 256;

    private final Set<String> selectedCapabilityIds = new LinkedHashSet<>();

    synchronized boolean update(
            CapabilityCatalogSnapshot catalog,
            String capabilityId,
            boolean selected) {
        if (catalog == null || capabilityId == null || capabilityId.isEmpty()
                || capabilityId.length() > MAX_CAPABILITY_ID_LENGTH) {
            return false;
        }
        CapabilitySnapshot capability = findCapability(catalog, capabilityId);
        if (capability == null || !isSelectable(capability)) {
            return false;
        }
        if (selected) {
            selectedCapabilityIds.add(capabilityId);
        } else {
            selectedCapabilityIds.remove(capabilityId);
        }
        return true;
    }

    synchronized Set<String> snapshot() {
        return Set.copyOf(selectedCapabilityIds);
    }

    static boolean isSelectable(CapabilitySnapshot capability) {
        return capability.adapterAttached()
                && capability.hardwarePresence() == HardwarePresence.PRESENT
                && capability.ports().stream().anyMatch(
                        port -> port.direction() == CapabilityPortDirection.SOURCE);
    }

    private static CapabilitySnapshot findCapability(
            CapabilityCatalogSnapshot catalog,
            String capabilityId) {
        for (CapabilitySnapshot capability : catalog.capabilities()) {
            if ((catalog.nodeId() + ":" + capability.capabilityKey()).equals(capabilityId)) {
                return capability;
            }
        }
        return null;
    }
}
