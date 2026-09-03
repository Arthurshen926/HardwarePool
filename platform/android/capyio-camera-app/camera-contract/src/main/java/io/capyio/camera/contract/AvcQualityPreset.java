package io.capyio.camera.contract;

/** Bounded foreground AVC bitrate choices that preserve the negotiated dimensions and frame rate. */
public enum AvcQualityPreset {
    ECONOMY(2_000_000),
    BALANCED(4_000_000),
    CLEAR(6_000_000);

    private static final long BASELINE_PIXELS = 1280L * 720L;

    private final int baselineBitrateBitsPerSecond;

    AvcQualityPreset(int baselineBitrateBitsPerSecond) {
        this.baselineBitrateBitsPerSecond = baselineBitrateBitsPerSecond;
    }

    public int bitrateForDimensions(int width, int height) {
        if (width <= 0 || height <= 0 || width > AvcEncoderConfig.MAX_DIMENSION
                || height > AvcEncoderConfig.MAX_DIMENSION) {
            throw new IllegalArgumentException("quality dimensions are outside the AVC bound");
        }
        long scaled = (long) baselineBitrateBitsPerSecond * width * height / BASELINE_PIXELS;
        long bounded = Math.max(
                AvcEncoderConfig.MIN_BITRATE_BITS_PER_SECOND,
                Math.min(AvcEncoderConfig.MAX_BITRATE_BITS_PER_SECOND, scaled));
        return (int) bounded;
    }

    public AvcQualityPreset next() {
        return switch (this) {
            case ECONOMY -> BALANCED;
            case BALANCED -> CLEAR;
            case CLEAR -> ECONOMY;
        };
    }
}
