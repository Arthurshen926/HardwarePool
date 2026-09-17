package dev.capyio.display.app;

public final class DisplayTlsOuterFrameHarness {
    private DisplayTlsOuterFrameHarness() {}

    public static void main(String[] args) {
        check(DisplayTlsOuterFrame.isHeartbeat(0, 1024), "zero length must be heartbeat");
        check(!DisplayTlsOuterFrame.isHeartbeat(1, 1024), "record length one is data");
        check(!DisplayTlsOuterFrame.isHeartbeat(1024, 1024), "maximum length is accepted");
        expectFailure(() -> DisplayTlsOuterFrame.isHeartbeat(-1, 1024));
        expectFailure(() -> DisplayTlsOuterFrame.isHeartbeat(1025, 1024));
        expectFailure(() -> DisplayTlsOuterFrame.isHeartbeat(0, 0));
    }

    private static void expectFailure(Runnable operation) {
        try {
            operation.run();
            throw new AssertionError("expected failure");
        } catch (IllegalArgumentException | SecurityException expected) {
            // Expected closed validation failure.
        }
    }

    private static void check(boolean condition, String message) {
        if (!condition) {
            throw new AssertionError(message);
        }
    }
}
