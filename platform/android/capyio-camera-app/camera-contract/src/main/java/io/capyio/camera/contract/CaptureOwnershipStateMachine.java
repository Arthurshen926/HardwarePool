package io.capyio.camera.contract;

/**
 * Pure lifecycle contract for moving camera ownership out of an Activity.
 *
 * <p>The foreground service owns capture. Activity visibility and configuration changes are UI
 * facts only and therefore cannot close a service-owned stream.
 */
public final class CaptureOwnershipStateMachine {
    public enum State {
        STOPPED,
        STARTING_SERVICE,
        SERVICE_OWNED,
        STOPPING,
        ERROR
    }

    public enum Event {
        USER_START_VISIBLE,
        SERVICE_STARTED,
        ACTIVITY_PAUSED,
        ACTIVITY_RESUMED,
        CONFIGURATION_CHANGED,
        USER_STOP,
        SERVICE_FAILED,
        SESSION_CLOSED
    }

    public enum Effect {
        NONE,
        START_FOREGROUND_SERVICE,
        STOP_SERVICE
    }

    public record Transition(State previous, State current, Effect effect) {}

    private State state = State.STOPPED;

    public State state() {
        return state;
    }

    public Transition handle(Event event) {
        if (event == null) {
            throw new IllegalArgumentException("event must be present");
        }
        State previous = state;
        Effect effect = Effect.NONE;
        switch (event) {
            case USER_START_VISIBLE -> {
                if (state == State.STOPPED || state == State.ERROR) {
                    state = State.STARTING_SERVICE;
                    effect = Effect.START_FOREGROUND_SERVICE;
                }
            }
            case SERVICE_STARTED -> {
                if (state == State.STARTING_SERVICE) {
                    state = State.SERVICE_OWNED;
                }
            }
            case ACTIVITY_PAUSED, ACTIVITY_RESUMED, CONFIGURATION_CHANGED -> {
                // Activity lifecycle is deliberately not capture lifecycle.
            }
            case USER_STOP -> {
                if (state == State.STARTING_SERVICE || state == State.SERVICE_OWNED) {
                    state = State.STOPPING;
                    effect = Effect.STOP_SERVICE;
                }
            }
            case SERVICE_FAILED -> {
                if (state == State.STARTING_SERVICE || state == State.SERVICE_OWNED) {
                    state = State.ERROR;
                    effect = Effect.STOP_SERVICE;
                }
            }
            case SESSION_CLOSED -> {
                if (state == State.STOPPING || state == State.ERROR) {
                    state = State.STOPPED;
                }
            }
        }
        return new Transition(previous, state, effect);
    }
}
