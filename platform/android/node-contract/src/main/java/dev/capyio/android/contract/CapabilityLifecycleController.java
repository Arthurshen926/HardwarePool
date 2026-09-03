package dev.capyio.android.contract;

/**
 * Generation-bound lifecycle authority for one non-audio Android Adapter.
 * Late platform callbacks cannot reactivate a stopped or retried generation.
 */
public final class CapabilityLifecycleController {
    private static final int MAX_PROBLEM_CODE_LENGTH = 96;

    private CapabilityLifecycleState state = CapabilityLifecycleState.STOPPED;
    private long generation;
    private String problemCode;

    public synchronized TransitionToken beginStart() {
        if (state != CapabilityLifecycleState.STOPPED
                && state != CapabilityLifecycleState.FAILED) {
            return new TransitionToken(false, generation);
        }
        generation = nextGeneration(generation);
        state = CapabilityLifecycleState.STARTING;
        problemCode = null;
        return new TransitionToken(true, generation);
    }

    public synchronized boolean completeStart(long expectedGeneration) {
        if (generation != expectedGeneration || state != CapabilityLifecycleState.STARTING) {
            return false;
        }
        state = CapabilityLifecycleState.ACTIVE;
        return true;
    }

    public synchronized TransitionToken beginStop() {
        if (state == CapabilityLifecycleState.STOPPED
                || state == CapabilityLifecycleState.STOPPING) {
            return new TransitionToken(false, generation);
        }
        generation = nextGeneration(generation);
        state = CapabilityLifecycleState.STOPPING;
        problemCode = null;
        return new TransitionToken(true, generation);
    }

    public synchronized boolean completeStop(long expectedGeneration) {
        if (generation != expectedGeneration || state != CapabilityLifecycleState.STOPPING) {
            return false;
        }
        state = CapabilityLifecycleState.STOPPED;
        return true;
    }

    public synchronized boolean fail(long expectedGeneration, String failureCode) {
        validateProblemCode(failureCode);
        if (generation != expectedGeneration
                || (state != CapabilityLifecycleState.STARTING
                        && state != CapabilityLifecycleState.ACTIVE)) {
            return false;
        }
        state = CapabilityLifecycleState.FAILED;
        problemCode = failureCode;
        return true;
    }

    public synchronized CapabilityLifecycleSnapshot snapshot() {
        return new CapabilityLifecycleSnapshot(state, generation, problemCode);
    }

    private static long nextGeneration(long current) {
        if (current == Long.MAX_VALUE) {
            throw new IllegalStateException("Capability generation exhausted");
        }
        return current + 1;
    }

    private static void validateProblemCode(String value) {
        if (value == null || value.isEmpty() || value.length() > MAX_PROBLEM_CODE_LENGTH) {
            throw new IllegalArgumentException("problem code is outside the supported bound");
        }
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            boolean valid = (character >= 'A' && character <= 'Z')
                    || (character >= '0' && character <= '9')
                    || character == '.'
                    || character == '_';
            if (!valid) {
                throw new IllegalArgumentException("problem code must be canonical ASCII");
            }
        }
    }
}
