package dev.capyio.android;

/** Explicit boundary for the existing insecure lab UDP adapters, not TLS dispatch. */
final class AndroidAudioRouteTransport {
    static final int SCHEMA_VERSION = 1;
    static final String LEGACY_UDP = "legacy-udp-v1";

    private AndroidAudioRouteTransport() {}

    /** Missing, unknown and secure transports fail before any socket or adapter setup. */
    static void requireLegacy(int version, String transport, String format, String direction) {
        String expected;
        if ("microphone".equals(direction)) {
            expected = "pcm-s16le-48000-mono";
        } else if ("speaker".equals(direction)) {
            expected = "pcm-s16le-48000-stereo";
        } else {
            throw new IllegalArgumentException("Unsupported audio direction");
        }
        if (version != SCHEMA_VERSION || !LEGACY_UDP.equals(transport) || !expected.equals(format)) {
            throw new IllegalArgumentException("Audio transport is not the explicit legacy UDP contract");
        }
    }
}
