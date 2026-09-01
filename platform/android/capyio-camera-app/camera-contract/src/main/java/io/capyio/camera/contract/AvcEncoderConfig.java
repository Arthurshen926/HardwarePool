package io.capyio.camera.contract;

/** Closed bootstrap configuration for one surface-input AVC encoder. */
public record AvcEncoderConfig(
        int width,
        int height,
        int framesPerSecond,
        int bitrateBitsPerSecond,
        int clockwiseRotationDegrees,
        int keyFrameIntervalSeconds,
        int queueCapacity) {
    public static final int MAX_DIMENSION = 4096;
    public static final int MAX_FRAMES_PER_SECOND = 60;
    public static final int MIN_BITRATE_BITS_PER_SECOND = 64_000;
    public static final int MAX_BITRATE_BITS_PER_SECOND = 50_000_000;
    public static final int MAX_KEY_FRAME_INTERVAL_SECONDS = 10;
    public static final int MAX_QUEUE_CAPACITY = 8;

    public AvcEncoderConfig {
        if (width <= 0
                || height <= 0
                || width > MAX_DIMENSION
                || height > MAX_DIMENSION
                || (width & 1) != 0
                || (height & 1) != 0) {
            throw new IllegalArgumentException("AVC dimensions must be positive bounded and even");
        }
        if (framesPerSecond <= 0 || framesPerSecond > MAX_FRAMES_PER_SECOND) {
            throw new IllegalArgumentException("AVC frame rate is outside the bootstrap bound");
        }
        if (bitrateBitsPerSecond < MIN_BITRATE_BITS_PER_SECOND
                || bitrateBitsPerSecond > MAX_BITRATE_BITS_PER_SECOND) {
            throw new IllegalArgumentException("AVC bitrate is outside the bootstrap bound");
        }
        if (clockwiseRotationDegrees != 0
                && clockwiseRotationDegrees != 90
                && clockwiseRotationDegrees != 180
                && clockwiseRotationDegrees != 270) {
            throw new IllegalArgumentException("AVC display rotation must be 0/90/180/270");
        }
        if (keyFrameIntervalSeconds <= 0
                || keyFrameIntervalSeconds > MAX_KEY_FRAME_INTERVAL_SECONDS) {
            throw new IllegalArgumentException("AVC key-frame interval is outside the bootstrap bound");
        }
        if (queueCapacity <= 0 || queueCapacity > MAX_QUEUE_CAPACITY) {
            throw new IllegalArgumentException("AVC queue capacity is outside the bootstrap bound");
        }
    }

    public static AvcEncoderConfig baseline720p30() {
        return new AvcEncoderConfig(1280, 720, 30, 4_000_000, 0, 1, 2);
    }
}
