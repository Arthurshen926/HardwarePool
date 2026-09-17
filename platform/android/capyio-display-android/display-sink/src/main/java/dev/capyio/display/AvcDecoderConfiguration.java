package dev.capyio.display;

import java.util.Arrays;
import java.util.Objects;
import java.util.UUID;

/** Versioned, bounded AVC decoder configuration; not a JNI memory layout. */
public final class AvcDecoderConfiguration {
    public static final int SCHEMA_VERSION = 1;
    public static final int MAX_DIMENSION_PIXELS = 8192;
    public static final int MAX_ACCESS_UNIT_BYTES = 16 * 1024 * 1024;
    public static final int MAX_CODEC_SPECIFIC_DATA_BYTES = 64 * 1024;
    private static final byte[] ANNEX_B_START_CODE = {0, 0, 0, 1};

    public enum Profile {
        BASELINE,
        MAIN,
        HIGH
    }

    public enum Level {
        L3_1,
        L4,
        L4_1,
        L4_2,
        L5,
        L5_1,
        L5_2
    }

    private final UUID streamId;
    private final long streamEpoch;
    private final int width;
    private final int height;
    private final int frameRateNumerator;
    private final int frameRateDenominator;
    private final int maximumAccessUnitBytes;
    private final Profile profile;
    private final Level level;
    private final byte[] sequenceParameterSet;
    private final byte[] pictureParameterSet;

    public AvcDecoderConfiguration(
            UUID streamId,
            long streamEpoch,
            int width,
            int height,
            int frameRateNumerator,
            int frameRateDenominator,
            int maximumAccessUnitBytes,
            Profile profile,
            Level level,
            byte[] sequenceParameterSet,
            byte[] pictureParameterSet) {
        this.streamId = Objects.requireNonNull(streamId, "streamId");
        this.streamEpoch = streamEpoch;
        this.width = width;
        this.height = height;
        this.frameRateNumerator = frameRateNumerator;
        this.frameRateDenominator = frameRateDenominator;
        this.maximumAccessUnitBytes = maximumAccessUnitBytes;
        this.profile = Objects.requireNonNull(profile, "profile");
        this.level = Objects.requireNonNull(level, "level");
        this.sequenceParameterSet = copyAndValidateCsd(
                "sequenceParameterSet", sequenceParameterSet, 7);
        this.pictureParameterSet = copyAndValidateCsd(
                "pictureParameterSet", pictureParameterSet, 8);
        validateScalarFields();
    }

    private void validateScalarFields() {
        if (streamEpoch <= 0) {
            throw new IllegalArgumentException("streamEpoch must be positive");
        }
        if (width <= 0 || width > MAX_DIMENSION_PIXELS
                || height <= 0 || height > MAX_DIMENSION_PIXELS) {
            throw new IllegalArgumentException("dimensions are outside the bounded display range");
        }
        if (frameRateNumerator <= 0 || frameRateDenominator <= 0) {
            throw new IllegalArgumentException("frame rate must be a positive rational");
        }
        if (maximumAccessUnitBytes <= 0
                || maximumAccessUnitBytes > MAX_ACCESS_UNIT_BYTES) {
            throw new IllegalArgumentException("maximumAccessUnitBytes exceeds the absolute cap");
        }
    }

    private static byte[] copyAndValidateCsd(String label, byte[] source, int nalUnitType) {
        Objects.requireNonNull(source, label);
        if (source.length <= ANNEX_B_START_CODE.length
                || source.length > MAX_CODEC_SPECIFIC_DATA_BYTES) {
            throw new IllegalArgumentException(label + " has an invalid size");
        }
        for (int index = 0; index < ANNEX_B_START_CODE.length; index++) {
            if (source[index] != ANNEX_B_START_CODE[index]) {
                throw new IllegalArgumentException(label + " lacks the Annex-B start code");
            }
        }
        if ((source[ANNEX_B_START_CODE.length] & 0x1f) != nalUnitType) {
            throw new IllegalArgumentException(label + " has the wrong AVC NAL unit type");
        }
        return Arrays.copyOf(source, source.length);
    }

    public UUID streamId() {
        return streamId;
    }

    public long streamEpoch() {
        return streamEpoch;
    }

    public int width() {
        return width;
    }

    public int height() {
        return height;
    }

    public int frameRateNumerator() {
        return frameRateNumerator;
    }

    public int frameRateDenominator() {
        return frameRateDenominator;
    }

    public float frameRate() {
        return (float) frameRateNumerator / (float) frameRateDenominator;
    }

    public int maximumAccessUnitBytes() {
        return maximumAccessUnitBytes;
    }

    public Profile profile() {
        return profile;
    }

    public Level level() {
        return level;
    }

    public byte[] sequenceParameterSetCopy() {
        return Arrays.copyOf(sequenceParameterSet, sequenceParameterSet.length);
    }

    public byte[] pictureParameterSetCopy() {
        return Arrays.copyOf(pictureParameterSet, pictureParameterSet.length);
    }
}
