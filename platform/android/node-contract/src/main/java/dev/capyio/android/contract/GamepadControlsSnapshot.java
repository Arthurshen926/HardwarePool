package dev.capyio.android.contract;

/** Complete bounded touch-gamepad state submitted by an Android UI surface. */
public record GamepadControlsSnapshot(
        int buttons,
        int dpadX,
        int dpadY,
        int leftX,
        int leftY,
        int rightX,
        int rightY,
        int leftTrigger,
        int rightTrigger) {
    public static final int SUPPORTED_BUTTON_MASK = (1 << 11) - 1;
    public static final int MIN_AXIS = -32_767;
    public static final int MAX_AXIS = 32_767;
    public static final int MAX_TRIGGER = 65_535;

    public GamepadControlsSnapshot {
        if (buttons < 0 || (buttons & ~SUPPORTED_BUTTON_MASK) != 0) {
            throw new IllegalArgumentException("gamepad buttons contain unsupported bits");
        }
        if (dpadX < -1 || dpadX > 1 || dpadY < -1 || dpadY > 1) {
            throw new IllegalArgumentException("gamepad dpad is outside the supported bound");
        }
        requireAxis(leftX);
        requireAxis(leftY);
        requireAxis(rightX);
        requireAxis(rightY);
        if (leftTrigger < 0 || leftTrigger > MAX_TRIGGER
                || rightTrigger < 0 || rightTrigger > MAX_TRIGGER) {
            throw new IllegalArgumentException("gamepad trigger is outside the supported bound");
        }
    }

    public static GamepadControlsSnapshot neutral() {
        return new GamepadControlsSnapshot(0, 0, 0, 0, 0, 0, 0, 0, 0);
    }

    private static void requireAxis(int value) {
        if (value < MIN_AXIS || value > MAX_AXIS) {
            throw new IllegalArgumentException("gamepad axis is outside the supported bound");
        }
    }
}
