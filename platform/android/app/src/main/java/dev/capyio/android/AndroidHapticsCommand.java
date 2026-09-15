package dev.capyio.android;

/** Bounded platform-neutral command admitted by the Android haptics Sink. */
public record AndroidHapticsCommand(
        long generation,
        int lowFrequency,
        int highFrequency,
        int durationMillis,
        boolean stop) {
    static final int MAX_MAGNITUDE = 1_000;
    static final int MAX_DURATION_MILLIS = 10_000;

    public AndroidHapticsCommand {
        if (generation <= 0) {
            throw new IllegalArgumentException("Haptics generation must be positive");
        }
        if (lowFrequency < 0 || lowFrequency > MAX_MAGNITUDE
                || highFrequency < 0 || highFrequency > MAX_MAGNITUDE) {
            throw new IllegalArgumentException("Haptics magnitude is outside 0..1000");
        }
        if (stop) {
            if (lowFrequency != 0 || highFrequency != 0 || durationMillis != 0) {
                throw new IllegalArgumentException("Stop must not carry rumble fields");
            }
        } else if (durationMillis <= 0 || durationMillis > MAX_DURATION_MILLIS
                || (lowFrequency == 0 && highFrequency == 0)) {
            throw new IllegalArgumentException("Rumble must be non-zero and bounded");
        }
    }

    static AndroidHapticsCommand rumble(
            long generation, int lowFrequency, int highFrequency, int durationMillis) {
        return new AndroidHapticsCommand(
                generation, lowFrequency, highFrequency, durationMillis, false);
    }

    static AndroidHapticsCommand stop(long generation) {
        return new AndroidHapticsCommand(generation, 0, 0, 0, true);
    }
}

