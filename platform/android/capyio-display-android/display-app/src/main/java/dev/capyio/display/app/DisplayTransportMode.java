package dev.capyio.display.app;

import java.util.Locale;

/** Explicitly selects one display transport without silently falling back. */
enum DisplayTransportMode {
    CPD1,
    GAMESTREAM;

    static DisplayTransportMode parse(String value) {
        if (value == null || value.isBlank()) {
            throw new IllegalArgumentException("display transport mode is missing");
        }
        return switch (value.trim().toLowerCase(Locale.ROOT)) {
            case "cpd1" -> CPD1;
            case "gamestream" -> GAMESTREAM;
            default -> throw new IllegalArgumentException(
                    "unsupported display transport mode: " + value);
        };
    }
}
