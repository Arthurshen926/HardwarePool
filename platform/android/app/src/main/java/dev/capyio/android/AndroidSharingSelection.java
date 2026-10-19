package dev.capyio.android;

import dev.capyio.android.contract.CapabilityCatalogSnapshot;
import dev.capyio.android.contract.CapabilityAvailability;
import dev.capyio.android.contract.CapabilityPortDirection;
import dev.capyio.android.contract.CapabilityPublicationProjection;
import dev.capyio.android.contract.CapabilitySnapshot;
import dev.capyio.android.contract.HardwarePresence;

import java.util.LinkedHashSet;
import java.util.Set;
import java.util.function.Consumer;

/** Service-owned publication intent for Android Sources and the speaker Sink (ADR 1504). */
final class AndroidSharingSelection {
    private static final int MAX_CAPABILITY_ID_LENGTH = 256;

    private final Set<String> selectedCapabilityIds = new LinkedHashSet<>();

    synchronized boolean update(
            CapabilityCatalogSnapshot catalog,
            String capabilityId,
            boolean selected) {
        return update(catalog, capabilityId, selected, ignored -> {});
    }

    synchronized boolean update(
            CapabilityCatalogSnapshot catalog,
            String capabilityId,
            boolean selected,
            Consumer<CapabilityCatalogSnapshot> beforeCommit) {
        if (catalog == null || capabilityId == null || capabilityId.isEmpty()
                || capabilityId.length() > MAX_CAPABILITY_ID_LENGTH || beforeCommit == null) {
            return false;
        }
        CapabilitySnapshot capability = findCapability(catalog, capabilityId);
        if (capability == null || !(hasSource(capability) || isSpeaker(capability))) {
            return false;
        }
        if (selected && !isSelectable(capability)) return false;
        Set<String> candidate = new LinkedHashSet<>(selectedCapabilityIds);
        if (selected) {
            candidate.add(capabilityId);
        } else {
            candidate.remove(capabilityId);
        }
        try {
            CapabilityCatalogSnapshot publication =
                    project(catalog, candidate);
            beforeCommit.accept(publication);
        } catch (RuntimeException invalid) {
            return false;
        }
        selectedCapabilityIds.clear();
        selectedCapabilityIds.addAll(candidate);
        return true;
    }

    synchronized Set<String> snapshot() {
        return Set.copyOf(selectedCapabilityIds);
    }

    synchronized CapabilityCatalogSnapshot publicationCatalog(
            CapabilityCatalogSnapshot catalog) {
        return project(catalog, selectedCapabilityIds);
    }

    static boolean isSelectable(CapabilitySnapshot capability) {
        return capability.adapterAttached()
                && capability.hardwarePresence() == HardwarePresence.PRESENT
                && capability.availability() != CapabilityAvailability.OFFLINE
                && (hasSource(capability) || isSpeaker(capability));
    }

    private static boolean isSpeaker(CapabilitySnapshot capability) {
        return capability.kind() == dev.capyio.android.contract.CapabilityKind.SPEAKER;
    }

    private static CapabilityCatalogSnapshot project(CapabilityCatalogSnapshot catalog, Set<String> selected) {
        Set<String> sources = new LinkedHashSet<>(selected);
        for (CapabilitySnapshot capability : catalog.capabilities()) {
            if (isSpeaker(capability)) sources.remove(catalog.nodeId() + ":" + capability.capabilityKey());
        }
        return CapabilityPublicationProjection.projectWithSpeakerSelection(catalog, sources,
                selected.contains(catalog.nodeId() + ":android.speaker"));
    }

    private static boolean hasSource(CapabilitySnapshot capability) {
        return capability.ports().stream().anyMatch(
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
