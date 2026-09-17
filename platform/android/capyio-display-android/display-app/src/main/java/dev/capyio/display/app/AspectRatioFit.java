package dev.capyio.display.app;

/** Computes a centered, aspect-preserving render size inside a bounded viewport. */
final class AspectRatioFit {
    record Size(int width, int height) {}

    private AspectRatioFit() {}

    static Size inside(int viewportWidth, int viewportHeight, int contentWidth, int contentHeight) {
        if (viewportWidth <= 0 || viewportHeight <= 0 || contentWidth <= 0 || contentHeight <= 0) {
            throw new IllegalArgumentException("aspect-ratio dimensions must be positive");
        }

        long widthLimitedHeight = (long) viewportWidth * contentHeight / contentWidth;
        if (widthLimitedHeight <= viewportHeight) {
            return new Size(viewportWidth, Math.max(1, (int) widthLimitedHeight));
        }

        long heightLimitedWidth = (long) viewportHeight * contentWidth / contentHeight;
        return new Size(Math.max(1, (int) heightLimitedWidth), viewportHeight);
    }
}
