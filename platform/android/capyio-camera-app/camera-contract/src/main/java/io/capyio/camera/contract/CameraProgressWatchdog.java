package io.capyio.camera.contract;

/** Fixed foreground bounds for detecting a configured camera stream that stops encoding. */
public final class CameraProgressWatchdog {
    public static final int CHECK_INTERVAL_MILLIS = 1_000;
    public static final int STALL_TIMEOUT_MILLIS = 5_000;

    private CameraProgressWatchdog() {}

    public static boolean isExpired(long nowMillis, long lastProgressMillis) {
        if (nowMillis < 0 || lastProgressMillis < 0 || nowMillis < lastProgressMillis) {
            throw new IllegalArgumentException("invalid monotonic watchdog timestamps");
        }
        return nowMillis - lastProgressMillis >= STALL_TIMEOUT_MILLIS;
    }
}
