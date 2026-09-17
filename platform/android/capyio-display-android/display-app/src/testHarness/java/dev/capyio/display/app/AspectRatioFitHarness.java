package dev.capyio.display.app;

public final class AspectRatioFitHarness {
    private AspectRatioFitHarness() {}

    public static void main(String[] args) {
        check(AspectRatioFit.inside(2640, 1216, 1024, 768), 1621, 1216,
                "4:3 content on the lab phone");
        check(AspectRatioFit.inside(1216, 2640, 1920, 1080), 1216, 684,
                "landscape content in a portrait viewport");
        check(AspectRatioFit.inside(1920, 1080, 1280, 720), 1920, 1080,
                "matching aspect ratios");
        check(AspectRatioFit.inside(2640, 1216, 1980, 912), 2640, 1216,
                "exact V2419A landscape aspect mode");

        try {
            AspectRatioFit.inside(0, 1080, 1280, 720);
            throw new AssertionError("zero-sized viewport must be rejected");
        } catch (IllegalArgumentException expected) {
            // Expected.
        }
    }

    private static void check(AspectRatioFit.Size actual, int width, int height, String context) {
        if (actual.width() != width || actual.height() != height) {
            throw new AssertionError(context + ": expected " + width + "x" + height
                    + ", got " + actual.width() + "x" + actual.height());
        }
    }
}
