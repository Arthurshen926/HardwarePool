package dev.capyio.android.contract;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/** Bounded registry owned by the Android Node service, not by an Activity. */
public final class CapabilityModuleRegistry {
    public static final int MAX_MODULES = 16;
    public static final int MAX_CAPABILITIES = 64;

    private final List<CapabilityModule> modules = new ArrayList<>();
    private final Set<String> moduleKeys = new HashSet<>();

    public synchronized void register(CapabilityModule module) {
        if (module == null) {
            throw new IllegalArgumentException("Capability module is required");
        }
        String moduleKey = CapabilitySnapshot.requireKey(module.moduleKey(), "module key");
        if (modules.size() == MAX_MODULES) {
            throw new IllegalStateException("Capability module registry is full");
        }
        if (!moduleKeys.add(moduleKey)) {
            throw new IllegalArgumentException("Capability module key is duplicated");
        }
        try {
            validateModuleSnapshot(module);
            modules.add(module);
        } catch (RuntimeException invalid) {
            moduleKeys.remove(moduleKey);
            throw invalid;
        }
    }

    public synchronized CapabilityCatalogSnapshot snapshot(String nodeId) {
        if (nodeId == null || nodeId.isEmpty() || nodeId.length() > 128) {
            throw new IllegalArgumentException("node ID is outside the supported bound");
        }
        List<CapabilitySnapshot> capabilities = new ArrayList<>();
        Set<String> capabilityKeys = new HashSet<>();
        for (CapabilityModule module : modules) {
            for (CapabilitySnapshot capability : validateModuleSnapshot(module)) {
                if (!capabilityKeys.add(capability.capabilityKey())) {
                    throw new IllegalStateException("Capability key is duplicated across modules");
                }
                capabilities.add(capability);
                if (capabilities.size() > MAX_CAPABILITIES) {
                    throw new IllegalStateException("Capability catalog is full");
                }
            }
        }
        return new CapabilityCatalogSnapshot(nodeId, modules.size(), capabilities);
    }

    private static List<CapabilitySnapshot> validateModuleSnapshot(CapabilityModule module) {
        List<CapabilitySnapshot> capabilities = module.capabilities();
        if (capabilities == null || capabilities.isEmpty()
                || capabilities.size() > MAX_CAPABILITIES) {
            throw new IllegalArgumentException("module Capability list is outside the supported bound");
        }
        List<CapabilitySnapshot> copied = List.copyOf(capabilities);
        long uniqueCount = copied.stream().map(CapabilitySnapshot::capabilityKey).distinct().count();
        if (uniqueCount != copied.size()) {
            throw new IllegalArgumentException("module Capability keys must be unique");
        }
        return copied;
    }
}
