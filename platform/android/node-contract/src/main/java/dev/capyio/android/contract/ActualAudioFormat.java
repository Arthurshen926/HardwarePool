package dev.capyio.android.contract;

import java.util.Objects;

/** Actual platform parameters observed after AudioRecord/AudioTrack initialization. */
public final class ActualAudioFormat {
    private static final int MIN_SAMPLE_RATE = 8_000;
    private static final int MAX_SAMPLE_RATE = 384_000;
    private static final int MAX_CHANNELS = 8;
    private static final int MAX_ENCODING_LENGTH = 32;

    private final int sampleRateHz;
    private final int channelCount;
    private final String encoding;
    private final int framesPerBuffer;

    public ActualAudioFormat(
            int sampleRateHz,
            int channelCount,
            String encoding,
            int framesPerBuffer) {
        if (sampleRateHz < MIN_SAMPLE_RATE || sampleRateHz > MAX_SAMPLE_RATE) {
            throw new IllegalArgumentException("sample rate is outside the supported bound");
        }
        if (channelCount < 1 || channelCount > MAX_CHANNELS) {
            throw new IllegalArgumentException("channel count is outside the supported bound");
        }
        if (!isCanonicalEncoding(encoding)) {
            throw new IllegalArgumentException("encoding must be a bounded canonical token");
        }
        if (framesPerBuffer < 1 || framesPerBuffer > sampleRateHz * 2) {
            throw new IllegalArgumentException("frames per buffer is outside the supported bound");
        }
        this.sampleRateHz = sampleRateHz;
        this.channelCount = channelCount;
        this.encoding = encoding;
        this.framesPerBuffer = framesPerBuffer;
    }

    private static boolean isCanonicalEncoding(String value) {
        if (value == null || value.isEmpty() || value.length() > MAX_ENCODING_LENGTH) {
            return false;
        }
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            boolean valid = (character >= 'a' && character <= 'z')
                    || (character >= '0' && character <= '9')
                    || character == '_'
                    || character == '-';
            if (!valid) {
                return false;
            }
        }
        return true;
    }

    public int sampleRateHz() {
        return sampleRateHz;
    }

    public int channelCount() {
        return channelCount;
    }

    public String encoding() {
        return encoding;
    }

    public int framesPerBuffer() {
        return framesPerBuffer;
    }

    @Override
    public boolean equals(Object other) {
        if (this == other) {
            return true;
        }
        if (!(other instanceof ActualAudioFormat)) {
            return false;
        }
        ActualAudioFormat that = (ActualAudioFormat) other;
        return sampleRateHz == that.sampleRateHz
                && channelCount == that.channelCount
                && framesPerBuffer == that.framesPerBuffer
                && encoding.equals(that.encoding);
    }

    @Override
    public int hashCode() {
        return Objects.hash(sampleRateHz, channelCount, encoding, framesPerBuffer);
    }
}
