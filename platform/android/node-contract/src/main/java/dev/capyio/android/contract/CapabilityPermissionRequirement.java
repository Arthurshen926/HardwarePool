package dev.capyio.android.contract;

/** Platform action still required before one capability can become active. */
public enum CapabilityPermissionRequirement {
    NONE,
    USER_CONFIRMATION,
    FOREGROUND_SERVICE,
    PRIVILEGED
}
