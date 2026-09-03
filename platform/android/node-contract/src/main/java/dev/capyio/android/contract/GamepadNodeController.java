package dev.capyio.android.contract;

/** Service-owned touch-gamepad lifecycle and latest complete control state. */
public final class GamepadNodeController {
    private final CapabilityLifecycleController lifecycle =
            new CapabilityLifecycleController();
    private GamepadControlsSnapshot controls = GamepadControlsSnapshot.neutral();

    public synchronized TransitionToken beginStart() {
        TransitionToken transition = lifecycle.beginStart();
        if (transition.accepted()) {
            controls = GamepadControlsSnapshot.neutral();
        }
        return transition;
    }

    public synchronized boolean completeStart(long generation) {
        return lifecycle.completeStart(generation);
    }

    public synchronized TransitionToken beginStop() {
        TransitionToken transition = lifecycle.beginStop();
        if (transition.accepted()) {
            controls = GamepadControlsSnapshot.neutral();
        }
        return transition;
    }

    public synchronized boolean completeStop(long generation) {
        return lifecycle.completeStop(generation);
    }

    public synchronized boolean fail(long generation, String problemCode) {
        boolean failed = lifecycle.fail(generation, problemCode);
        if (failed) {
            controls = GamepadControlsSnapshot.neutral();
        }
        return failed;
    }

    public synchronized boolean submit(long generation, GamepadControlsSnapshot next) {
        if (next == null) {
            throw new IllegalArgumentException("gamepad controls are required");
        }
        CapabilityLifecycleSnapshot current = lifecycle.snapshot();
        if (current.generation() != generation
                || current.state() != CapabilityLifecycleState.ACTIVE) {
            return false;
        }
        controls = next;
        return true;
    }

    public synchronized CapabilityLifecycleSnapshot lifecycle() {
        return lifecycle.snapshot();
    }

    public synchronized GamepadControlsSnapshot controls() {
        return controls;
    }
}
