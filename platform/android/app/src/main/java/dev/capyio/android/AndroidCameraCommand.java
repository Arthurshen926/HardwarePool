package dev.capyio.android;

import dev.capyio.android.contract.CapabilityLifecycleState;

/** Closed command grammar for the shared UI camera Quick Action. */
final class AndroidCameraCommand {
    static final String ACTION_ID = "capyio.quick-action.android-camera";

    enum Operation { START, STOP, RETRY }

    private final Operation operation;

    private AndroidCameraCommand(Operation operation) {
        this.operation = operation;
    }

    Operation operation() {
        return operation;
    }

    static AndroidCameraCommand parse(
            String actionId,
            String operation,
            CapabilityLifecycleState state) {
        if (!ACTION_ID.equals(actionId) || operation == null || state == null) {
            return null;
        }
        Operation parsed = switch (operation) {
            case "start" -> Operation.START;
            case "stop" -> Operation.STOP;
            case "retry" -> Operation.RETRY;
            default -> null;
        };
        if (parsed == null) {
            return null;
        }
        boolean allowed = switch (state) {
            case STOPPED -> parsed == Operation.START;
            case STARTING, ACTIVE -> parsed == Operation.STOP;
            case STOPPING -> false;
            case FAILED -> parsed == Operation.RETRY;
        };
        return allowed ? new AndroidCameraCommand(parsed) : null;
    }
}
