package dev.capyio.android.contract;

public enum AudioCapabilityState {
    STOPPED,
    STARTING,
    ACTIVE,
    STOPPING,
    FAILED;

    public boolean ownsForegroundLifecycle() {
        return this == STARTING || this == ACTIVE || this == STOPPING;
    }
}
