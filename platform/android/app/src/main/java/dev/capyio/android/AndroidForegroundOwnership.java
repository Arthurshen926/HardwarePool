package dev.capyio.android;

import dev.capyio.android.contract.CapabilityLifecycleState;

/** Pure policy for deciding whether a non-audio Route keeps the Node service alive. */
final class AndroidForegroundOwnership {
    private AndroidForegroundOwnership() {}

    static boolean connectedDevice(
            CapabilityLifecycleState imu,
            CapabilityLifecycleState gamepad,
            CapabilityLifecycleState touchpad,
            CapabilityLifecycleState haptics) {
        return owns(imu) || owns(gamepad) || owns(touchpad) || owns(haptics);
    }

    static boolean connectedDevice(
            boolean publication,
            boolean backgroundWaiting,
            CapabilityLifecycleState imu,
            CapabilityLifecycleState gamepad,
            CapabilityLifecycleState touchpad,
            CapabilityLifecycleState haptics) {
        return publication || backgroundWaiting
                || connectedDevice(imu, gamepad, touchpad, haptics);
    }

    private static boolean owns(CapabilityLifecycleState state) {
        return state != null && state.ownsAdapterLifecycle();
    }
}
