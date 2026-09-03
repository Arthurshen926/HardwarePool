package io.capyio.controllerlab;

final class ControllerState {
    static final int SOUTH = 1 << 0;
    static final int EAST = 1 << 1;
    static final int WEST = 1 << 2;
    static final int NORTH = 1 << 3;
    static final int LEFT_SHOULDER = 1 << 4;
    static final int RIGHT_SHOULDER = 1 << 5;
    static final int LEFT_STICK = 1 << 6;
    static final int RIGHT_STICK = 1 << 7;
    static final int SELECT = 1 << 8;
    static final int START = 1 << 9;
    static final int GUIDE = 1 << 10;

    private int buttons;
    private int dpadX;
    private int dpadY;
    private int leftX;
    private int leftY;
    private int rightX;
    private int rightY;
    private int leftTrigger;
    private int rightTrigger;
    private final float[] acceleration = {0.0f, 0.0f, 9.80665f};
    private final float[] angularVelocity = {0.0f, 0.0f, 0.0f};
    private long accelerationTimestampNanos = 1;
    private long angularVelocityTimestampNanos = 1;

    synchronized void setButton(int button, boolean pressed) {
        buttons = pressed ? buttons | button : buttons & ~button;
    }

    synchronized void setDpad(int x, int y) {
        dpadX = clampUnit(x);
        dpadY = clampUnit(y);
    }

    synchronized void setStick(boolean left, int x, int y) {
        if (left) {
            leftX = clampAxis(x);
            leftY = clampAxis(y);
        } else {
            rightX = clampAxis(x);
            rightY = clampAxis(y);
        }
    }

    synchronized void setTrigger(boolean left, int value) {
        if (left) {
            leftTrigger = clampTrigger(value);
        } else {
            rightTrigger = clampTrigger(value);
        }
    }

    synchronized void setAcceleration(float x, float y, float z, long timestampNanos) {
        acceleration[0] = finiteOrZero(x);
        acceleration[1] = finiteOrZero(y);
        acceleration[2] = finiteOrZero(z);
        accelerationTimestampNanos = Math.max(1L, timestampNanos);
    }

    synchronized void setAngularVelocity(float x, float y, float z, long timestampNanos) {
        angularVelocity[0] = finiteOrZero(x);
        angularVelocity[1] = finiteOrZero(y);
        angularVelocity[2] = finiteOrZero(z);
        angularVelocityTimestampNanos = Math.max(1L, timestampNanos);
    }

    synchronized void resetControls() {
        buttons = 0;
        dpadX = 0;
        dpadY = 0;
        leftX = 0;
        leftY = 0;
        rightX = 0;
        rightY = 0;
        leftTrigger = 0;
        rightTrigger = 0;
    }

    synchronized Snapshot snapshot() {
        return new Snapshot(
                buttons, dpadX, dpadY,
                leftX, leftY, rightX, rightY,
                leftTrigger, rightTrigger,
                acceleration.clone(), angularVelocity.clone(),
                accelerationTimestampNanos, angularVelocityTimestampNanos);
    }

    private static int clampUnit(int value) {
        return Math.max(-1, Math.min(1, value));
    }

    private static int clampAxis(int value) {
        return Math.max(-32767, Math.min(32767, value));
    }

    private static int clampTrigger(int value) {
        return Math.max(0, Math.min(65535, value));
    }

    private static float finiteOrZero(float value) {
        return Float.isFinite(value) ? value : 0.0f;
    }

    static final class Snapshot {
        private final int buttons;
        private final int dpadX;
        private final int dpadY;
        private final int leftX;
        private final int leftY;
        private final int rightX;
        private final int rightY;
        private final int leftTrigger;
        private final int rightTrigger;
        private final float[] acceleration;
        private final float[] angularVelocity;
        private final long accelerationTimestampNanos;
        private final long angularVelocityTimestampNanos;

        Snapshot(
                int buttons, int dpadX, int dpadY,
                int leftX, int leftY, int rightX, int rightY,
                int leftTrigger, int rightTrigger,
                float[] acceleration, float[] angularVelocity,
                long accelerationTimestampNanos, long angularVelocityTimestampNanos) {
            this.buttons = buttons;
            this.dpadX = dpadX;
            this.dpadY = dpadY;
            this.leftX = leftX;
            this.leftY = leftY;
            this.rightX = rightX;
            this.rightY = rightY;
            this.leftTrigger = leftTrigger;
            this.rightTrigger = rightTrigger;
            this.acceleration = acceleration;
            this.angularVelocity = angularVelocity;
            this.accelerationTimestampNanos = accelerationTimestampNanos;
            this.angularVelocityTimestampNanos = angularVelocityTimestampNanos;
        }

        int buttons() { return buttons; }
        int dpadX() { return dpadX; }
        int dpadY() { return dpadY; }
        int leftX() { return leftX; }
        int leftY() { return leftY; }
        int rightX() { return rightX; }
        int rightY() { return rightY; }
        int leftTrigger() { return leftTrigger; }
        int rightTrigger() { return rightTrigger; }
        float[] acceleration() { return acceleration; }
        float[] angularVelocity() { return angularVelocity; }
        long accelerationTimestampNanos() { return accelerationTimestampNanos; }
        long angularVelocityTimestampNanos() { return angularVelocityTimestampNanos; }
    }
}
