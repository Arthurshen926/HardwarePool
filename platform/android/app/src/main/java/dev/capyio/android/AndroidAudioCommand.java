package dev.capyio.android;

import dev.capyio.android.contract.AudioCapabilityKind;
import dev.capyio.android.contract.AudioCapabilityState;

/** Closed shared-UI command for the two existing Android audio capabilities. */
public record AndroidAudioCommand(AudioCapabilityKind target, Operation operation) {
    public enum Operation {
        START,
        RETRY,
        STOP
    }

    public AndroidAudioCommand {
        if (target == null || operation == null) {
            throw new IllegalArgumentException("Audio command fields are required");
        }
    }

    public static AndroidAudioCommand parse(
            String actionId, String operation, AudioCapabilityState state) {
        if (actionId == null || operation == null || state == null
                || actionId.length() > 96 || operation.length() > 8) {
            return null;
        }
        AudioCapabilityKind target;
        if ("capyio.quick-action.android-microphone".equals(actionId)) {
            target = AudioCapabilityKind.MICROPHONE_SOURCE;
        } else if ("capyio.quick-action.android-speaker".equals(actionId)) {
            target = AudioCapabilityKind.SPEAKER_SINK;
        } else {
            return null;
        }
        Operation parsed = switch (operation) {
            case "start" -> Operation.START;
            case "retry" -> Operation.RETRY;
            case "stop" -> Operation.STOP;
            default -> null;
        };
        if (parsed == null || !allowed(state, parsed)) {
            return null;
        }
        return new AndroidAudioCommand(target, parsed);
    }

    private static boolean allowed(AudioCapabilityState state, Operation operation) {
        return switch (state) {
            case STOPPED -> operation == Operation.START;
            case STARTING, ACTIVE -> operation == Operation.STOP;
            case STOPPING -> false;
            case FAILED -> operation == Operation.RETRY;
        };
    }
}
