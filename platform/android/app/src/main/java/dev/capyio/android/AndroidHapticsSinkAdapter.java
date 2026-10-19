package dev.capyio.android;

import android.os.VibrationEffect;
import android.os.Vibrator;

import dev.capyio.android.contract.CapabilityLifecycleController;
import dev.capyio.android.contract.CapabilityLifecycleSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleState;
import dev.capyio.android.contract.TransitionToken;

/** Service-owned single-actuator projection of the dual-rumble Sink contract. */
final class AndroidHapticsSinkAdapter {
    private static final String PROBLEM_HARDWARE = "CAPY.ANDROID.HAPTICS_HARDWARE_MISSING";
    private static final String PROBLEM_START = "CAPY.ANDROID.HAPTICS_START_FAILED";
    private static final String PROBLEM_WRITE = "CAPY.ANDROID.HAPTICS_WRITE_FAILED";
    private static final int CONFIRMATION_DURATION_MILLIS = 35;
    private static final int CONFIRMATION_AMPLITUDE = 72;

    private final Vibrator vibrator;
    private final CapabilityLifecycleController controller;
    private final Runnable stateChanged;
    private volatile int strengthPercent = 70;

    boolean available() { return vibrator != null && vibrator.hasVibrator(); }

    int strengthPercent() { return strengthPercent; }

    void setStrengthPercent(int percent) {
        if (percent < 0 || percent > 100) throw new IllegalArgumentException("Strength must be 0..100");
        strengthPercent = percent;
        // Do not let an effect at the previous gain continue after a change.
        if (vibrator != null) vibrator.cancel();
    }

    AndroidHapticsSinkAdapter(
            Vibrator vibrator,
            CapabilityLifecycleController controller,
            Runnable stateChanged) {
        this.vibrator = vibrator;
        this.controller = controller;
        this.stateChanged = stateChanged;
    }

    synchronized void start() { start(true); }

    synchronized long beginRemote() {
        if (controller.snapshot().state().ownsAdapterLifecycle()) return 0;
        start(false);
        CapabilityLifecycleSnapshot state = controller.snapshot();
        return state.state() == CapabilityLifecycleState.ACTIVE ? state.generation() : 0;
    }

    synchronized void endRemote(long generation) {
        if (controller.snapshot().generation() == generation) stop();
    }

    private void start(boolean confirmation) {
        TransitionToken transition = controller.beginStart();
        if (!transition.accepted()) {
            stateChanged.run();
            return;
        }
        if (vibrator == null) {
            controller.fail(transition.generation(), PROBLEM_HARDWARE);
            stateChanged.run();
            return;
        }
        try {
            if (!vibrator.hasVibrator()) {
                controller.fail(transition.generation(), PROBLEM_HARDWARE);
                stateChanged.run();
                return;
            }
            int amplitude = HapticsStrength.amplitude(
                    CONFIRMATION_AMPLITUDE * 1000 / 255, strengthPercent);
            if (confirmation && amplitude > 0) vibrator.vibrate(VibrationEffect.createOneShot(
                    CONFIRMATION_DURATION_MILLIS, amplitude));
            controller.completeStart(transition.generation());
        } catch (RuntimeException denied) {
            controller.fail(transition.generation(), PROBLEM_START);
        }
        stateChanged.run();
    }

    synchronized void stop() {
        TransitionToken transition = controller.beginStop();
        if (transition.accepted()) {
            try {
                if (vibrator != null) {
                    vibrator.cancel();
                }
            } catch (RuntimeException ignored) {
                // Lifecycle still closes; there is no safe recovery action during Stop.
            }
            controller.completeStop(transition.generation());
        }
        stateChanged.run();
    }

    void failForeground(String problemCode) {
        CapabilityLifecycleSnapshot lifecycle = controller.snapshot();
        if (!lifecycle.state().ownsAdapterLifecycle()) {
            return;
        }
        try {
            if (vibrator != null) {
                vibrator.cancel();
            }
        } catch (RuntimeException ignored) {
            // The lifecycle failure remains authoritative if cancellation is denied.
        }
        controller.fail(lifecycle.generation(), problemCode);
    }

    synchronized boolean submit(AndroidHapticsCommand command) {
        if (command == null) {
            throw new IllegalArgumentException("Haptics command is required");
        }
        CapabilityLifecycleSnapshot lifecycle = controller.snapshot();
        if (lifecycle.state() != CapabilityLifecycleState.ACTIVE
                || lifecycle.generation() != command.generation()
                || vibrator == null) {
            return false;
        }
        try {
            if (command.stop()) {
                vibrator.cancel();
            } else {
                int magnitude = Math.max(command.lowFrequency(), command.highFrequency());
                int amplitude = HapticsStrength.amplitude(magnitude, strengthPercent);
                if (amplitude == 0) vibrator.cancel();
                else vibrator.vibrate(VibrationEffect.createOneShot(
                        command.durationMillis(), amplitude));
            }
            return true;
        } catch (RuntimeException denied) {
            controller.fail(command.generation(), PROBLEM_WRITE);
            stateChanged.run();
            return false;
        }
    }
}
