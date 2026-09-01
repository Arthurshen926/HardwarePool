package io.capyio.camera.contract;

import java.util.Objects;

/**
 * Deterministic lifecycle guard for one user-visible Android camera session.
 *
 * <p>The owner drives this object only from its UI thread. Camera callbacks are
 * marshalled to that thread before they become events.</p>
 */
public final class CaptureStateMachine {
    public enum State {
        IDLE,
        AWAITING_PERMISSION,
        STARTING,
        STREAMING,
        STOPPING,
        STOPPED,
        ERROR
    }

    public enum Event {
        USER_START_WITH_PERMISSION,
        USER_START_WITHOUT_PERMISSION,
        PERMISSION_GRANTED,
        PERMISSION_DENIED,
        SESSION_STARTED,
        USER_STOP,
        HOST_PAUSED,
        SESSION_CLOSED,
        FAILURE
    }

    public enum Effect {
        NONE,
        REQUEST_PERMISSION,
        OPEN_CAMERA,
        CLOSE_CAMERA
    }

    public record Transition(State previous, State current, Effect effect) {}

    private State state = State.IDLE;

    public State state() {
        return state;
    }

    public Transition handle(Event event) {
        Objects.requireNonNull(event, "event");
        State previous = state;
        Effect effect = Effect.NONE;

        switch (event) {
            case USER_START_WITH_PERMISSION -> {
                if (canStart()) {
                    state = State.STARTING;
                    effect = Effect.OPEN_CAMERA;
                }
            }
            case USER_START_WITHOUT_PERMISSION -> {
                if (canStart()) {
                    state = State.AWAITING_PERMISSION;
                    effect = Effect.REQUEST_PERMISSION;
                }
            }
            case PERMISSION_GRANTED -> {
                if (state == State.AWAITING_PERMISSION) {
                    state = State.STARTING;
                    effect = Effect.OPEN_CAMERA;
                }
            }
            case PERMISSION_DENIED -> {
                if (state == State.AWAITING_PERMISSION) {
                    state = State.STOPPED;
                }
            }
            case SESSION_STARTED -> {
                if (state == State.STARTING) {
                    state = State.STREAMING;
                }
            }
            case USER_STOP, HOST_PAUSED -> {
                if (state == State.STARTING || state == State.STREAMING || state == State.ERROR) {
                    state = State.STOPPING;
                    effect = Effect.CLOSE_CAMERA;
                } else if (state == State.AWAITING_PERMISSION) {
                    state = State.STOPPED;
                }
            }
            case SESSION_CLOSED -> {
                if (state == State.STOPPING || state == State.STARTING) {
                    state = State.STOPPED;
                }
            }
            case FAILURE -> {
                if (state == State.STARTING || state == State.STREAMING) {
                    state = State.ERROR;
                    effect = Effect.CLOSE_CAMERA;
                }
            }
        }

        return new Transition(previous, state, effect);
    }

    private boolean canStart() {
        return state == State.IDLE || state == State.STOPPED || state == State.ERROR;
    }
}
