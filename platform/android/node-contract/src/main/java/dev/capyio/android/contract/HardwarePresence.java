package dev.capyio.android.contract;

/** Result of a read-only platform inventory probe; it does not imply Adapter readiness. */
public enum HardwarePresence {
    PRESENT,
    ABSENT,
    UNKNOWN
}
