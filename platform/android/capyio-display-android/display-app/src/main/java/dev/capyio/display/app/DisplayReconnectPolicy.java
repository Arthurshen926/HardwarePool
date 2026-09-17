package dev.capyio.display.app;

import java.security.GeneralSecurityException;
import javax.net.ssl.SSLHandshakeException;

/** Deterministic bounded retry schedule for transient display transport loss. */
final class DisplayReconnectPolicy {
    static final int MAX_ATTEMPTS = 8;
    private static final long INITIAL_DELAY_MILLIS = 250;
    private static final long MAX_DELAY_MILLIS = 4_000;

    private DisplayReconnectPolicy() {}

    static long delayMillis(int attempt) {
        if (attempt < 1 || attempt > MAX_ATTEMPTS) {
            throw new IllegalArgumentException("display reconnect attempt is outside its bound");
        }
        int shifts = Math.min(attempt - 1, 4);
        return Math.min(INITIAL_DELAY_MILLIS << shifts, MAX_DELAY_MILLIS);
    }

    static boolean isPermanentLaunchFailure(Throwable error) {
        Throwable current = error;
        for (int depth = 0; current != null && depth < 6; depth++) {
            if (current instanceof SecurityException
                    || current instanceof GeneralSecurityException
                    || current instanceof SSLHandshakeException) {
                return true;
            }
            current = current.getCause();
        }
        return false;
    }

    static boolean isTransientGameStreamResult(int result) {
        // Positive values are platform socket errors. The three negative values
        // are Moonlight Common C's transport/video-liveness failures.
        return result > 0 || result == -100 || result == -101 || result == -102;
    }
}
