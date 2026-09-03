package dev.capyio.android.contract;

import java.util.List;

/** Narrow observer façade implemented by each Android hardware module. */
public interface CapabilityModule {
    String moduleKey();

    List<CapabilitySnapshot> capabilities();
}
