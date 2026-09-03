package dev.capyio.android.contract;

import java.util.List;

/** Versioned catalog entry; it contains no platform handle or high-rate payload. */
public final class CapabilitySnapshot {
    private static final int MAX_KEY_LENGTH = 96;
    private static final int MAX_DISPLAY_NAME_LENGTH = 128;
    private static final int MAX_PORTS = 8;

    private final String capabilityKey;
    private final String displayName;
    private final CapabilityKind kind;
    private final CapabilityAvailability availability;
    private final CapabilityPermissionRequirement permissionRequirement;
    private final HardwarePresence hardwarePresence;
    private final boolean adapterAttached;
    private final List<CapabilityPortSnapshot> ports;
    private final String problemCode;

    public CapabilitySnapshot(
            String capabilityKey,
            String displayName,
            CapabilityKind kind,
            CapabilityAvailability availability,
            CapabilityPermissionRequirement permissionRequirement,
            HardwarePresence hardwarePresence,
            boolean adapterAttached,
            List<CapabilityPortSnapshot> ports,
            String problemCode) {
        this.capabilityKey = requireKey(capabilityKey, "Capability key");
        this.displayName = requireDisplayName(displayName, "Capability display name");
        if (kind == null || availability == null || permissionRequirement == null
                || hardwarePresence == null) {
            throw new IllegalArgumentException("Capability enums are required");
        }
        if (!adapterAttached && availability != CapabilityAvailability.OFFLINE) {
            throw new IllegalArgumentException("an unattached Adapter must remain offline");
        }
        if (hardwarePresence != HardwarePresence.PRESENT
                && availability != CapabilityAvailability.OFFLINE) {
            throw new IllegalArgumentException("non-present hardware must remain offline");
        }
        if (ports == null || ports.isEmpty() || ports.size() > MAX_PORTS) {
            throw new IllegalArgumentException("Capability Ports are outside the supported bound");
        }
        List<CapabilityPortSnapshot> copiedPorts = List.copyOf(ports);
        long uniquePortCount = copiedPorts.stream().map(CapabilityPortSnapshot::portKey).distinct().count();
        if (uniquePortCount != copiedPorts.size()) {
            throw new IllegalArgumentException("Capability Port keys must be unique");
        }
        this.kind = kind;
        this.availability = availability;
        this.permissionRequirement = permissionRequirement;
        this.hardwarePresence = hardwarePresence;
        this.adapterAttached = adapterAttached;
        this.ports = copiedPorts;
        this.problemCode = problemCode == null ? null : requireProblemCode(problemCode);
    }

    public String capabilityKey() {
        return capabilityKey;
    }

    public String displayName() {
        return displayName;
    }

    public CapabilityKind kind() {
        return kind;
    }

    public CapabilityAvailability availability() {
        return availability;
    }

    public CapabilityPermissionRequirement permissionRequirement() {
        return permissionRequirement;
    }

    public HardwarePresence hardwarePresence() {
        return hardwarePresence;
    }

    public boolean adapterAttached() {
        return adapterAttached;
    }

    public List<CapabilityPortSnapshot> ports() {
        return ports;
    }

    public String problemCode() {
        return problemCode;
    }

    static String requireKey(String value, String field) {
        if (value == null || value.isEmpty() || value.length() > MAX_KEY_LENGTH) {
            throw new IllegalArgumentException(field + " is outside the supported bound");
        }
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            boolean valid = (character >= 'a' && character <= 'z')
                    || (character >= '0' && character <= '9')
                    || character == '.'
                    || character == '_'
                    || character == '-';
            if (!valid) {
                throw new IllegalArgumentException(field + " must be canonical lowercase ASCII");
            }
        }
        return value;
    }

    static String requireDisplayName(String value, String field) {
        if (value == null || value.isBlank() || value.length() > MAX_DISPLAY_NAME_LENGTH) {
            throw new IllegalArgumentException(field + " is outside the supported bound");
        }
        return value;
    }

    static String requireProfile(String value) {
        if (value == null) {
            throw new IllegalArgumentException("Port profile is required");
        }
        int separator = value.lastIndexOf('/');
        if (separator < 1 || separator == value.length() - 1) {
            throw new IllegalArgumentException("Port profile must contain a positive major version");
        }
        requireKey(value.substring(0, separator), "profile name");
        String major = value.substring(separator + 1);
        if (major.charAt(0) == '0') {
            throw new IllegalArgumentException("Port profile major version must be canonical");
        }
        for (int index = 0; index < major.length(); index++) {
            char character = major.charAt(index);
            if (character < '0' || character > '9') {
                throw new IllegalArgumentException("Port profile major version must be numeric");
            }
        }
        try {
            if (Integer.parseInt(major) < 1) {
                throw new IllegalArgumentException("Port profile major version must be positive");
            }
        } catch (NumberFormatException invalid) {
            throw new IllegalArgumentException("Port profile major version must be numeric", invalid);
        }
        return value;
    }

    private static String requireProblemCode(String value) {
        if (value.isEmpty() || value.length() > MAX_KEY_LENGTH) {
            throw new IllegalArgumentException("problem code is outside the supported bound");
        }
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            boolean valid = (character >= 'A' && character <= 'Z')
                    || (character >= '0' && character <= '9')
                    || character == '.'
                    || character == '_';
            if (!valid) {
                throw new IllegalArgumentException("problem code must be canonical uppercase ASCII");
            }
        }
        return value;
    }
}
