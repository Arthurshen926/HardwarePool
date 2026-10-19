package dev.capyio.android.contract;

import java.util.List;

/** Service-owned touchpad lifecycle and latest complete contact frame. */
public final class TouchpadNodeController {
    private final CapabilityLifecycleController lifecycle =
            new CapabilityLifecycleController();
    private TouchpadControlsSnapshot controls = TouchpadControlsSnapshot.neutral(0);

    public synchronized TransitionToken beginStart() {
        TransitionToken transition = lifecycle.beginStart();
        if (transition.accepted()) {
            controls = TouchpadControlsSnapshot.neutral(0);
        }
        return transition;
    }

    public synchronized boolean completeStart(long generation) {
        return lifecycle.completeStart(generation);
    }

    public synchronized TransitionToken beginStop() {
        TransitionToken transition = lifecycle.beginStop();
        if (transition.accepted()) {
            controls = TouchpadControlsSnapshot.neutral(nextSequence());
        }
        return transition;
    }

    public synchronized boolean completeStop(long generation) {
        return lifecycle.completeStop(generation);
    }

    /** Terminal worker completion without a local UI Stop; stale workers cannot stop retries. */
    public synchronized boolean completeRemoteStop(long generation) {
        CapabilityLifecycleSnapshot current = lifecycle.snapshot();
        if (current.generation() != generation
                || (current.state() != CapabilityLifecycleState.ACTIVE
                    && current.state() != CapabilityLifecycleState.STARTING)) return false;
        TransitionToken stop = beginStop();
        return stop.accepted() && completeStop(stop.generation());
    }

    public synchronized boolean fail(long generation, String problemCode) {
        boolean failed = lifecycle.fail(generation, problemCode);
        if (failed) {
            controls = TouchpadControlsSnapshot.neutral(nextSequence());
        }
        return failed;
    }

    public synchronized boolean submit(
            long generation, List<TouchpadContactSnapshot> contacts) {
        CapabilityLifecycleSnapshot current = lifecycle.snapshot();
        if (current.generation() != generation
                || current.state() != CapabilityLifecycleState.ACTIVE) {
            return false;
        }
        controls = new TouchpadControlsSnapshot(nextSequence(), contacts);
        return true;
    }

    public synchronized CapabilityLifecycleSnapshot lifecycle() {
        return lifecycle.snapshot();
    }

    public synchronized TouchpadControlsSnapshot controls() {
        return controls;
    }

    private long nextSequence() {
        if (controls.sequence() == Long.MAX_VALUE) {
            throw new IllegalStateException("touchpad sequence is exhausted");
        }
        return controls.sequence() + 1;
    }
}
