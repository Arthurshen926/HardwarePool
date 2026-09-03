package dev.capyio.android;

import dev.capyio.android.contract.GamepadControlsSnapshot;

/** Applies one closed shared-UI command to a complete bounded gamepad snapshot. */
public final class AndroidGamepadUiCommand {
    private AndroidGamepadUiCommand() {}

    public static GamepadControlsSnapshot apply(
            String kind,
            String target,
            int x,
            int y,
            boolean pressed,
            GamepadControlsSnapshot current) {
        if (kind == null || target == null || current == null) {
            return null;
        }
        return switch (kind) {
            case "button" -> applyButton(target, x, y, pressed, current);
            case "dpad" -> applyDpad(target, x, y, pressed, current);
            case "stick" -> applyStick(target, x, y, pressed, current);
            case "trigger" -> applyTrigger(target, x, y, pressed, current);
            case "reset" -> target.isEmpty() && x == 0 && y == 0 && !pressed
                    ? GamepadControlsSnapshot.neutral() : null;
            default -> null;
        };
    }

    private static GamepadControlsSnapshot applyButton(
            String target, int x, int y, boolean pressed, GamepadControlsSnapshot current) {
        int mask = buttonMask(target);
        if (mask == 0 || x != 0 || y != 0) {
            return null;
        }
        int buttons = pressed ? current.buttons() | mask : current.buttons() & ~mask;
        return copy(buttons, current.dpadX(), current.dpadY(),
                current.leftX(), current.leftY(), current.rightX(), current.rightY(),
                current.leftTrigger(), current.rightTrigger());
    }

    private static GamepadControlsSnapshot applyDpad(
            String target, int x, int y, boolean pressed, GamepadControlsSnapshot current) {
        if (!target.isEmpty() || pressed || x < -1 || x > 1 || y < -1 || y > 1) {
            return null;
        }
        return copy(current.buttons(), x, y,
                current.leftX(), current.leftY(), current.rightX(), current.rightY(),
                current.leftTrigger(), current.rightTrigger());
    }

    private static GamepadControlsSnapshot applyStick(
            String target, int x, int y, boolean pressed, GamepadControlsSnapshot current) {
        if (pressed || x < GamepadControlsSnapshot.MIN_AXIS
                || x > GamepadControlsSnapshot.MAX_AXIS
                || y < GamepadControlsSnapshot.MIN_AXIS
                || y > GamepadControlsSnapshot.MAX_AXIS) {
            return null;
        }
        if ("left".equals(target)) {
            return copy(current.buttons(), current.dpadX(), current.dpadY(),
                    x, y, current.rightX(), current.rightY(),
                    current.leftTrigger(), current.rightTrigger());
        }
        if ("right".equals(target)) {
            return copy(current.buttons(), current.dpadX(), current.dpadY(),
                    current.leftX(), current.leftY(), x, y,
                    current.leftTrigger(), current.rightTrigger());
        }
        return null;
    }

    private static GamepadControlsSnapshot applyTrigger(
            String target, int value, int unused, boolean pressed,
            GamepadControlsSnapshot current) {
        if (pressed || unused != 0 || value < 0 || value > GamepadControlsSnapshot.MAX_TRIGGER) {
            return null;
        }
        if ("left".equals(target)) {
            return copy(current.buttons(), current.dpadX(), current.dpadY(),
                    current.leftX(), current.leftY(), current.rightX(), current.rightY(),
                    value, current.rightTrigger());
        }
        if ("right".equals(target)) {
            return copy(current.buttons(), current.dpadX(), current.dpadY(),
                    current.leftX(), current.leftY(), current.rightX(), current.rightY(),
                    current.leftTrigger(), value);
        }
        return null;
    }

    private static GamepadControlsSnapshot copy(
            int buttons,
            int dpadX,
            int dpadY,
            int leftX,
            int leftY,
            int rightX,
            int rightY,
            int leftTrigger,
            int rightTrigger) {
        return new GamepadControlsSnapshot(
                buttons, dpadX, dpadY, leftX, leftY, rightX, rightY,
                leftTrigger, rightTrigger);
    }

    private static int buttonMask(String target) {
        return switch (target) {
            case "south" -> 1 << 0;
            case "east" -> 1 << 1;
            case "west" -> 1 << 2;
            case "north" -> 1 << 3;
            case "left_shoulder" -> 1 << 4;
            case "right_shoulder" -> 1 << 5;
            case "left_stick" -> 1 << 6;
            case "right_stick" -> 1 << 7;
            case "select" -> 1 << 8;
            case "start" -> 1 << 9;
            case "guide" -> 1 << 10;
            default -> 0;
        };
    }
}
