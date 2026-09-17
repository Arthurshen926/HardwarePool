package dev.capyio.display.app;

/** Availability is not authentication, a running Route or presented-frame proof. */
public record DisplayPrerequisites(String transport, boolean configurationPresent,
        boolean nativeClientAvailable) {
    public DisplayPrerequisites {
        if (!"cpd1".equals(transport) && !"gamestream".equals(transport)
                && !"unconfigured".equals(transport)) {
            throw new IllegalArgumentException("Unknown display transport");
        }
    }

    public boolean canOpen() {
        return configurationPresent && !"unconfigured".equals(transport)
                && (!"gamestream".equals(transport) || nativeClientAvailable);
    }

    public String problemCode() {
        if (!configurationPresent || "unconfigured".equals(transport)) {
            return "CAPY.DISPLAY.CONFIGURATION_REQUIRED";
        }
        if (!canOpen()) return "CAPY.DISPLAY.NATIVE_CLIENT_MISSING";
        return "";
    }
}
