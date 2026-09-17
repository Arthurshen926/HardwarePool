package dev.capyio.display.app;

public final class DisplayTransportModeHarness {
    private DisplayTransportModeHarness() {}

    public static void main(String[] args) {
        check(DisplayTransportMode.parse("cpd1") == DisplayTransportMode.CPD1,
                "CPD1 mode mismatch");
        check(DisplayTransportMode.parse(" GameStream ") == DisplayTransportMode.GAMESTREAM,
                "GameStream mode mismatch");
        expectFailure(() -> DisplayTransportMode.parse(""));
        expectFailure(() -> DisplayTransportMode.parse("auto"));
        expectFailure(() -> DisplayTransportMode.parse(null));
    }

    private static void expectFailure(Runnable action) {
        try {
            action.run();
            throw new AssertionError("expected mode rejection");
        } catch (IllegalArgumentException expected) {
            // Expected.
        }
    }

    private static void check(boolean condition, String message) {
        if (!condition) {
            throw new AssertionError(message);
        }
    }
}
