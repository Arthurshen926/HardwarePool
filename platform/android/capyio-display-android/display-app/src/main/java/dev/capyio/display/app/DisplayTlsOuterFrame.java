package dev.capyio.display.app;

/** Closed validation for the TLS display stream's four-byte outer length. */
final class DisplayTlsOuterFrame {
    static final int HEARTBEAT_LENGTH = 0;

    private DisplayTlsOuterFrame() {}

    static boolean isHeartbeat(int length, int maximumRecordBytes) {
        if (maximumRecordBytes <= 0) {
            throw new IllegalArgumentException("maximum record length must be positive");
        }
        if (length < 0 || length > maximumRecordBytes) {
            throw new SecurityException("TLS display record length is outside its bound");
        }
        return length == HEARTBEAT_LENGTH;
    }
}
