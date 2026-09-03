package dev.capyio.android.contract;

/** Runtime availability, separate from sharing selection and Route authorization. */
public enum CapabilityAvailability {
    AVAILABLE,
    BUSY,
    PERMISSION_REQUIRED,
    OFFLINE
}
