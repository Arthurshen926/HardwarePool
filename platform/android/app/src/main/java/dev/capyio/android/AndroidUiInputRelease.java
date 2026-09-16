package dev.capyio.android;

import dev.capyio.android.contract.CapabilityLifecycleSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleState;
import dev.capyio.android.contract.GamepadControlsSnapshot;
import dev.capyio.android.contract.TouchpadContactSnapshot;
import dev.capyio.android.contract.TouchpadControlsSnapshot;

import java.util.List;

/** Pure policy for releasing UI-owned input without stopping its Service lifecycle. */
final class AndroidUiInputRelease {
    private AndroidUiInputRelease() {}

    static GamepadControlsSnapshot gamepad(
            CapabilityLifecycleSnapshot lifecycle,
            GamepadControlsSnapshot controls) {
        GamepadControlsSnapshot neutral = GamepadControlsSnapshot.neutral();
        return lifecycle.state() == CapabilityLifecycleState.ACTIVE && !neutral.equals(controls)
                ? neutral
                : null;
    }

    static List<TouchpadContactSnapshot> touchpad(
            CapabilityLifecycleSnapshot lifecycle,
            TouchpadControlsSnapshot controls) {
        return lifecycle.state() == CapabilityLifecycleState.ACTIVE
                        && !controls.contacts().isEmpty()
                ? List.of()
                : null;
    }
}
