package dev.capyio.android.contract;

/** Hardware-free lifecycle states shared by non-audio Android capability Adapters. */
public enum CapabilityLifecycleState {
    STOPPED,
    STARTING,
    ACTIVE,
    STOPPING,
    FAILED;

    public boolean ownsAdapterLifecycle() {
        return this == STARTING || this == ACTIVE || this == STOPPING;
    }
}
