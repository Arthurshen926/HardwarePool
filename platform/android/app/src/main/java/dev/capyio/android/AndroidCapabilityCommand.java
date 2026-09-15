package dev.capyio.android;

import dev.capyio.android.contract.CapabilityLifecycleState;

/** Closed UI-to-Service command admitted for the A01D local Capability slice. */
public record AndroidCapabilityCommand(Target target, Operation operation) {
    public enum Target {
        IMU,
        GAMEPAD,
        TOUCHPAD,
        HAPTICS
    }

    public enum Operation {
        START,
        RETRY,
        STOP
    }

    public AndroidCapabilityCommand {
        if (target == null || operation == null) {
            throw new IllegalArgumentException("Capability command fields are required");
        }
    }

    public static AndroidCapabilityCommand parse(
            String actionId,
            String operation,
            CapabilityLifecycleState state) {
        if (actionId == null || operation == null || state == null
                || actionId.length() > 96 || operation.length() > 8) {
            return null;
        }
        Target target;
        if ("capyio.quick-action.android-imu".equals(actionId)) {
            target = Target.IMU;
        } else if ("capyio.quick-action.android-gamepad".equals(actionId)) {
            target = Target.GAMEPAD;
        } else if ("capyio.quick-action.android-touchpad".equals(actionId)) {
            target = Target.TOUCHPAD;
        } else if ("capyio.quick-action.android-haptics".equals(actionId)) {
            target = Target.HAPTICS;
        } else {
            return null;
        }
        Operation parsedOperation = switch (operation) {
            case "start" -> Operation.START;
            case "retry" -> Operation.RETRY;
            case "stop" -> Operation.STOP;
            default -> null;
        };
        if (parsedOperation == null || !allowed(state, parsedOperation)) {
            return null;
        }
        return new AndroidCapabilityCommand(target, parsedOperation);
    }

    private static boolean allowed(CapabilityLifecycleState state, Operation operation) {
        return switch (state) {
            case STOPPED -> operation == Operation.START;
            case STARTING, ACTIVE -> operation == Operation.STOP;
            case STOPPING -> false;
            case FAILED -> operation == Operation.RETRY;
        };
    }
}
