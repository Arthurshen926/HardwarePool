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

    AndroidHapticsSinkAdapter(
            Vibrator vibrator,
            CapabilityLifecycleController controller,
            Runnable stateChanged) {
        this.vibrator = vibrator;
        this.controller = controller;
        this.stateChanged = stateChanged;
    }

    void start() {
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
            vibrator.vibrate(VibrationEffect.createOneShot(
                    CONFIRMATION_DURATION_MILLIS, CONFIRMATION_AMPLITUDE));
            controller.completeStart(transition.generation());
        } catch (RuntimeException denied) {
            controller.fail(transition.generation(), PROBLEM_START);
        }
        stateChanged.run();
    }

    void stop() {
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

    boolean submit(AndroidHapticsCommand command) {
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
                int amplitude = Math.max(1, (magnitude * 255 + 999) / 1_000);
                vibrator.vibrate(VibrationEffect.createOneShot(
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
