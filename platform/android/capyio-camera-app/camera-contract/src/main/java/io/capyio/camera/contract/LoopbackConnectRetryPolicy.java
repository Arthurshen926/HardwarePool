package io.capyio.camera.contract;

/** Fixed foreground-lab retry bounds for a receiver that starts after capture. */
public final class LoopbackConnectRetryPolicy {
    public static final int MAX_ATTEMPTS = 120;
    public static final int CONNECT_TIMEOUT_MILLIS = 500;
    public static final int RETRY_DELAY_MILLIS = 500;

    private LoopbackConnectRetryPolicy() {}

    public static boolean mayAttempt(int attemptNumber) {
        return attemptNumber >= 1 && attemptNumber <= MAX_ATTEMPTS;
    }

    public static boolean shouldRetryAfterFailure(int attemptNumber) {
        if (!mayAttempt(attemptNumber)) {
            throw new IllegalArgumentException("connect attempt is outside the retry bound");
        }
        return attemptNumber < MAX_ATTEMPTS;
    }
}
