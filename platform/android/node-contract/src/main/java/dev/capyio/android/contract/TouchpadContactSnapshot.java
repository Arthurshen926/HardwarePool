package dev.capyio.android.contract;

/** One normalized contact in a complete Android touchpad frame. */
public record TouchpadContactSnapshot(int contactId, int x, int y, int pressure) {
    public static final int MAX_COORDINATE = 10_000;
    public static final int MAX_PRESSURE = 1_024;

    public TouchpadContactSnapshot {
        if (contactId < 0 || contactId >= TouchpadControlsSnapshot.MAX_CONTACTS) {
            throw new IllegalArgumentException("touchpad contact ID is outside the supported bound");
        }
        if (x < 0 || x > MAX_COORDINATE || y < 0 || y > MAX_COORDINATE) {
            throw new IllegalArgumentException("touchpad coordinate is outside the supported bound");
        }
        if (pressure < 0 || pressure > MAX_PRESSURE) {
            throw new IllegalArgumentException("touchpad pressure is outside the supported bound");
        }
    }
}
