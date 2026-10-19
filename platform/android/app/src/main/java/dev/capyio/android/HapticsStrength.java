package dev.capyio.android;

/** Local actuator gain, independent of the remote command's normalized values. */
final class HapticsStrength {
    private HapticsStrength() {}

    static int amplitude(int magnitude, int percent) {
        if (magnitude < 0 || magnitude > 1000 || percent < 0 || percent > 100) {
            throw new IllegalArgumentException("Invalid haptics magnitude or strength");
        }
        if (magnitude == 0 || percent == 0) return 0;
        return Math.max(1, (magnitude * percent * 255 + 99999) / 100000);
    }
}
